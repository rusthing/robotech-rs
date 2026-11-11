//! # 基于 NATS JetStream 的刷新信号
//!
//! 信号本身没有业务含义，只表示「数据变了，去看一眼」。发布走 JetStream
//! 持久化，每个节点一个独立 Push Consumer（名字里带实例 ID），保证各副本
//! 都能收到，而不是被其中一个抢走。
//!
//! ## 与缓存的关系
//!
//! 本模块只负责「收信号、执行 handler、发现订阅断了就重建」。
//! 缓存是它的第一个使用方，但 handler 可以是任意动作。
//!
//! ## 合并
//!
//! 信号只表示「数据变了」，多条信号对应的全量重建结果完全相同。因此订阅
//! 回调不在收到信号时直接执行动作，只登记「有变更」；由守护任务等
//! `[refresh] debounce-window`（默认 3s）静默后统一执行一次，持续写入时
//! 最长不超过 `max-wait`（默认 15s）。批量导入、一次写发多个主题、
//! 运维脚本连发多个主题、消息重投都因此收敛成一次重建。
//!
//! 代价是 ACK 提前到「登记成功」这一刻：窗口内进程退出，这条信号不会再
//! 投递，靠重启后的首次加载与 `rebuild-interval` 兜底收敛——与「信号可以
//! 丢、动作全量重建幂等」的既有约定一致。

use async_nats::jetstream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{error, info, warn};

use crate::mq::nats::{get_or_create_stream, publish as nats_publish, subscribe as nats_subscribe};

use super::config::get_refresh_config;
use super::subject::RefreshSubject;

/// 已注册的订阅：subject → 守护任务句柄
///
/// 用于保证同一主题只注册一次——配置热更新会重新执行 `setup()`，
/// 业务代码重复调用 [`subscribe`] 时不能叠加订阅和守护任务。
static REGISTERED: LazyLock<Mutex<HashMap<RefreshSubject, Arc<JoinHandle<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// # 刷新信号负载
///
/// 内容不被消费方使用，仅用于满足 JetStream 需要非空负载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshSignal {
    /// 动作标识，固定为 `refresh`
    pub action: String,
}

impl RefreshSignal {
    pub fn new() -> Self {
        Self {
            action: "refresh".to_string(),
        }
    }
}

impl Default for RefreshSignal {
    fn default() -> Self {
        Self::new()
    }
}

/// # 订阅刷新信号
///
/// 收到信号后按合并窗口（`[refresh] debounce-window`，默认 3s；最长等
/// `max-wait`）执行**一次** `handler`，窗口内的多条信号只算一次；consumer
/// 丢失时自动重建订阅并立即执行一次 `handler`，把中断期间漏掉的变更补齐。
///
/// 同一 `subject` 重复调用只会注册一次，返回同一个守护任务句柄。
///
/// ## 参数
///
/// - `subject`：刷新信号主题（只能由 [`RefreshSubject`] 构造，保证命名合规）
/// - `handler`：收到信号后执行的异步动作，内部应做「全量重建」而不是增量更新
///
/// ## 返回值
///
/// 返回守护任务句柄；`abort()` 会同时停掉订阅与探活。
pub async fn subscribe<H, Fut>(
    subject: &RefreshSubject,
    handler: H,
) -> anyhow::Result<Arc<JoinHandle<()>>>
where
    H: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let mut registered = REGISTERED.lock().await;

    if let Some(handle) = registered.get(subject) {
        info!("刷新订阅已注册，复用现有订阅: {subject}");
        return Ok(Arc::clone(handle));
    }

    let stream_config = default_stream_config(subject);
    let consumer_name = format!(
        "{}_consumer_{}",
        stream_config.name,
        crate::env::get_app_instance_id()
    );
    let consumer_config = default_consumer_config(&consumer_name);

    let watchdog = install(subject, stream_config, consumer_config, handler).await?;
    registered.insert(*subject, Arc::clone(&watchdog));

    Ok(watchdog)
}

/// # 发布刷新信号
///
/// 通过 JetStream 持久化发布，所有订阅该主题的节点收到后各自执行刷新动作。
///
/// 用于 SVC 写操作成功后的 `after_write` 回调。
pub async fn publish(subject: &RefreshSubject) -> anyhow::Result<()> {
    nats_publish(subject.as_str(), &RefreshSignal::new(), true).await?;
    Ok(())
}

/// # 安装订阅与守护（不幂等）
///
/// 供 [`subscribe`] 与缓存层的兼容入口使用。
///
/// 合并窗口的通知通道在这里创建：`install` 是唯一的装配点，
/// `cache::local::nats::setup_nats_cache` 等兼容入口也走它，放在这层才能
/// 让所有调用方都享受合并。
pub(crate) async fn install<H, Fut>(
    subject: &RefreshSubject,
    stream_config: jetstream::stream::Config,
    consumer_config: jetstream::consumer::push::Config,
    handler: H,
) -> anyhow::Result<Arc<JoinHandle<()>>>
where
    H: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let handler = Arc::new(handler);

    // 容量 1 足够：只表达「有未执行的变更」，满了说明已经登记过
    let (signal_tx, signal_rx) = mpsc::channel::<()>(1);

    let handle = open_subscription(
        subject,
        stream_config.clone(),
        consumer_config.clone(),
        signal_tx.clone(),
    )
    .await?;

    Ok(Arc::new(spawn_watchdog(
        *subject,
        stream_config,
        consumer_config,
        handler,
        handle,
        signal_tx,
        signal_rx,
    )))
}

/// # 建立一次订阅
///
/// 先清理同名 consumer，保证新建的 consumer 不重放历史信号（启动与重连都走这里）。
///
/// 回调只把「有变更」登记到合并窗口并立即返回，不在这里执行 handler：
/// 执行时机由 [`spawn_watchdog`] 按窗口决定。
async fn open_subscription(
    subject: &RefreshSubject,
    stream_config: jetstream::stream::Config,
    consumer_config: jetstream::consumer::push::Config,
    signal_tx: mpsc::Sender<()>,
) -> anyhow::Result<Arc<JoinHandle<()>>> {
    if let Some(consumer_name) = consumer_config.name.as_ref() {
        cleanup_stale_consumer(&stream_config, consumer_name).await;
    }

    let handle = nats_subscribe(
        subject.as_str(),
        None,
        Some(stream_config),
        Some(consumer_config),
        move |_msg| {
            let signal_tx = signal_tx.clone();
            async move {
                // 只登记「有变更」，重建由守护任务在窗口结束时做一次。
                // 队列满说明已有待处理信号，再登记一次没有意义。
                // 立即返回 Ok 让 JetStream 尽早 ACK：动作是全量重建，窗口内
                // 进程退出由启动加载与兜底重建收敛，不需要重投。
                let _ = signal_tx.try_send(());
                Ok(())
            }
        },
    )
    .await?;

    info!("刷新订阅已建立 (subject={subject})");
    Ok(handle)
}

/// # 待执行的合并窗口
///
/// 记录第一条与最后一条信号的时间：窗口从**最后一条**信号起算 `debounce`，
/// 但硬上限是**第一条**信号加 `max_wait`。持续写入只顺延前者，不会让缓存
/// 无限期停在旧快照。
#[derive(Debug, Default, Clone, Copy)]
struct RefreshWindow {
    /// 窗口内第一条信号的时间，决定最大等待的起点
    first_at: Option<Instant>,
    /// 窗口内最后一条信号的时间，决定静默到期时间
    last_at: Option<Instant>,
}

impl RefreshWindow {
    /// 登记一条信号：窗口未开则起点即此刻，已开只顺延末条时间
    fn on_signal(&mut self, now: Instant) {
        self.first_at.get_or_insert(now);
        self.last_at = Some(now);
    }

    /// 下一次应当执行动作的时间
    ///
    /// `debounce` 为 `None`（关闭合并）时返回首条信号时间，即「立即执行」。
    fn deadline(&self, debounce: Option<Duration>, max_wait: Duration) -> Option<Instant> {
        let first = self.first_at?;
        let last = self.last_at?;

        Some(match debounce {
            None => first,
            Some(window) => (last + window).min(first + max_wait.max(window)),
        })
    }

    /// 当前是否已到执行时间
    fn is_due(&self, now: Instant, debounce: Option<Duration>, max_wait: Duration) -> bool {
        self.deadline(debounce, max_wait)
            .is_some_and(|deadline| now >= deadline)
    }

    /// 清空窗口（执行过全量重建后，之前登记的变更都已被覆盖）
    fn clear(&mut self) {
        self.first_at = None;
        self.last_at = None;
    }
}

/// # 订阅守护任务
///
/// 一个 subject 一个任务，负责四件事：
/// 1. 收合并窗口的登记信号，按 `[refresh] debounce-window` / `max-wait`
///    决定何时真正执行一次 `handler`；
/// 2. 检查客户端订阅任务是否已退出（`handle.is_finished()`）；
/// 3. 探活服务端 consumer 是否还在；
/// 4. 即便订阅健全，也按 `[refresh] rebuild-interval` 周期性执行一次 `handler`。
///
/// 第 2、3 项任一不成立就重建订阅并立即执行一次 `handler`，两个检查缺一不可：
/// consumer 还在但消费任务已退出时，只看服务端会误判为「一切正常」，
/// 而实际上已经没有人在收消息了。
///
/// 第 4 项兜的是「订阅一切正常、但根本没有信号」的情况：发布侧漏发通知、
/// 信号被别的 Stream 吞掉、数据由外部写入等，都不会让 consumer 消失，
/// 光靠探活永远发现不了，缓存会一直停在旧快照。动作是全量重建，重复执行
/// 幂等；`rebuild-interval = "0s"` 可关闭。
///
/// 探活补刷与兜底重建都会清空窗口：它们查到的就是最新数据，窗口里登记的
/// 变更已被覆盖，不必再执行一次。
fn spawn_watchdog<H, Fut>(
    subject: RefreshSubject,
    stream_config: jetstream::stream::Config,
    consumer_config: jetstream::consumer::push::Config,
    handler: Arc<H>,
    initial_handle: Arc<JoinHandle<()>>,
    signal_tx: mpsc::Sender<()>,
    mut signal_rx: mpsc::Receiver<()>,
) -> JoinHandle<()>
where
    H: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let consumer_name = consumer_config.name.clone().unwrap_or_default();

    tokio::spawn(async move {
        let mut handle = initial_handle;
        let mut last_rebuild = Instant::now();
        let mut next_probe = Instant::now() + get_refresh_config().effective_probe_interval();
        // 空表示当前没有未执行的变更
        let mut window = RefreshWindow::default();

        loop {
            // 每轮取一次，配置热更新后下一轮即生效
            let config = get_refresh_config();
            let probe_interval = config.effective_probe_interval();
            let rebuild_interval = config.effective_rebuild_interval();
            let debounce = config.effective_debounce_window();
            let max_wait = config.effective_max_wait();

            // 三件事都可能先到：探活、合并窗口到期、兜底重建
            let mut wake_at = next_probe;
            if let Some(deadline) = window.deadline(debounce, max_wait) {
                wake_at = wake_at.min(deadline);
            }
            if let Some(interval) = rebuild_interval {
                wake_at = wake_at.min(last_rebuild + interval);
            }
            let now = Instant::now();
            if wake_at < now {
                wake_at = now;
            }

            tokio::select! {
                signal = signal_rx.recv() => {
                    // 只登记，不在这里执行：下面统一判断窗口是否到期
                    if signal.is_some() {
                        window.on_signal(Instant::now());
                    }
                }
                _ = tokio::time::sleep_until(wake_at) => {}
            }

            let now = Instant::now();

            // 1) 合并窗口到期：整批信号只执行一次
            if window.is_due(now, debounce, max_wait) {
                window.clear();
                // 失败只记日志：动作是全量重建，下一次信号或兜底会覆盖
                if let Err(e) = handler().await {
                    error!("刷新失败: {e}");
                }
                last_rebuild = Instant::now();
            }

            // 2) 探活
            if now >= next_probe {
                next_probe = Instant::now() + probe_interval;

                let mut needs_resubscribe = false;
                if handle.is_finished() {
                    warn!("刷新订阅任务已退出，重建订阅 (consumer={consumer_name})");
                    needs_resubscribe = true;
                } else {
                    match consumer_alive(&stream_config, &consumer_name).await {
                        Ok(true) => {}
                        Ok(false) => {
                            warn!("刷新消费者 {consumer_name} 已丢失，重建订阅");
                            needs_resubscribe = true;
                        }
                        Err(e) => {
                            warn!("刷新消费者探活失败，稍后重试: {e}");
                            continue;
                        }
                    }
                }

                if needs_resubscribe {
                    handle.abort();

                    match open_subscription(
                        &subject,
                        stream_config.clone(),
                        consumer_config.clone(),
                        signal_tx.clone(),
                    )
                    .await
                    {
                        Ok(new_handle) => {
                            handle = new_handle;
                            info!("刷新订阅已重建 (consumer={consumer_name})");

                            // 中断期间发出的信号已经丢了，直接补一次刷新；
                            // 窗口里的变更也被这次全量重建覆盖
                            window.clear();
                            if let Err(e) = handler().await {
                                error!("重建后补刷新失败: {e}");
                            }
                            last_rebuild = Instant::now();
                        }
                        Err(e) => error!("重建刷新订阅失败，下一轮重试: {e}"),
                    }
                    continue;
                }
            }

            // 3) 订阅健全但长期没有信号：周期性全量重建，把「发布侧漏发」这类
            // 静默不一致限制在一个间隔内。重建是幂等的，与信号重复执行无副作用。
            if let Some(interval) = rebuild_interval
                && last_rebuild.elapsed() >= interval
            {
                window.clear();
                if let Err(e) = handler().await {
                    error!("刷新订阅兜底重建失败: {e}");
                }
                last_rebuild = Instant::now();
            }
        }
    })
}

/// # 默认 Stream 配置
///
/// - `subject`：刷新信号主题，Stream 名由主题派生（`.` 替换为 `_`）
/// - `subjects` 精确匹配该主题，保证与发布主题一致
/// - 文件持久化，7 天 TTL
pub fn default_stream_config(subject: &RefreshSubject) -> jetstream::stream::Config {
    jetstream::stream::Config {
        name: subject.stream_name(),
        subjects: vec![subject.as_str().to_string()],
        storage: jetstream::stream::StorageType::File,
        max_messages: -1,
        max_bytes: -1,
        max_age: Duration::from_secs(604800),
        ..Default::default()
    }
}

/// # 默认 Push Consumer 配置
///
/// - `DeliverPolicy::New`：新建消费者不重放历史信号
/// - `AckPolicy::Explicit`：处理完成后 ACK
/// - `name`：consumer 名，含节点实例 ID，保证多副本各自独立、互不抢消息
/// - 不设置 `durable_name`：consumer 为具名 ephemeral，进程退出后由服务端
///   按 `inactive_threshold` 回收，不会随重启累积元数据
pub fn default_consumer_config(consumer_name: &str) -> jetstream::consumer::push::Config {
    jetstream::consumer::push::Config {
        name: Some(consumer_name.to_string()),
        deliver_policy: jetstream::consumer::DeliverPolicy::New,
        ack_policy: jetstream::consumer::AckPolicy::Explicit,
        ..Default::default()
    }
}

/// # 探测 Push Consumer 是否存活
///
/// - `Ok(true)`：stream 与 consumer 都还在，订阅正常
/// - `Ok(false)`：二者之一已不存在，需要重建订阅
/// - `Err`：无法判断（如 NATS 不可达），调用方应保持现状并稍后重试，
///   避免网络抖动触发无谓的重建
pub async fn consumer_alive(
    stream_config: &jetstream::stream::Config,
    consumer_name: &str,
) -> anyhow::Result<bool> {
    let stream = get_or_create_stream(stream_config.clone()).await?;

    match stream.consumer_info(consumer_name).await {
        Ok(_) => Ok(true),
        Err(e)
            if matches!(
                e.kind(),
                async_nats::jetstream::context::ConsumerInfoErrorKind::NotFound
                    | async_nats::jetstream::context::ConsumerInfoErrorKind::StreamNotFound
            ) =>
        {
            Ok(false)
        }
        Err(e) => Err(anyhow::anyhow!(
            "探测 consumer {consumer_name} 失败: {e}"
        )),
    }
}

/// # 清理旧的 Push Consumer
///
/// 订阅前删除同名 consumer，确保不重放历史信号。
/// 删除失败不影响后续流程（consumer 可能不存在或权限不足）。
async fn cleanup_stale_consumer(
    stream_config: &jetstream::stream::Config,
    consumer_name: &str,
) {
    match get_or_create_stream(stream_config.clone()).await {
        Ok(stream) => {
            if let Err(e) = stream.delete_consumer(consumer_name).await {
                if e.to_string().contains("not found") {
                    info!("旧 consumer 不存在，无需清理: {consumer_name}");
                } else {
                    error!("删除旧 consumer 失败: {e}");
                }
            } else {
                info!("已清理旧 consumer: {consumer_name}");
            }
        }
        Err(e) => {
            error!("获取 Stream 失败，跳过 consumer 清理: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEBOUNCE: Duration = Duration::from_secs(3);
    const MAX_WAIT: Duration = Duration::from_secs(15);

    #[test]
    fn window_extends_from_last_signal() {
        let base = Instant::now();
        let mut window = RefreshWindow::default();
        window.on_signal(base);
        window.on_signal(base + Duration::from_millis(500));

        assert_eq!(
            window.deadline(Some(DEBOUNCE), MAX_WAIT),
            Some(base + Duration::from_millis(3_500)),
            "窗口从最后一条信号起算，不是第一条"
        );
    }

    #[test]
    fn max_wait_caps_continuous_signals() {
        let base = Instant::now();
        let mut window = RefreshWindow::default();
        window.on_signal(base);
        // 每 2 秒一条，窗口一直顺延，但 15 秒处必须执行
        for step in 1..=10 {
            window.on_signal(base + Duration::from_secs(step * 2));
        }

        assert_eq!(
            window.deadline(Some(DEBOUNCE), MAX_WAIT),
            Some(base + MAX_WAIT),
            "持续写入不能让缓存一直停在旧快照"
        );
        assert!(!window.is_due(
            base + MAX_WAIT - Duration::from_millis(1),
            Some(DEBOUNCE),
            MAX_WAIT
        ));
        assert!(window.is_due(base + MAX_WAIT, Some(DEBOUNCE), MAX_WAIT));
    }

    #[test]
    fn window_is_not_due_before_deadline() {
        let base = Instant::now();
        let mut window = RefreshWindow::default();
        window.on_signal(base);

        assert!(!window.is_due(base, Some(DEBOUNCE), MAX_WAIT));
        assert!(!window.is_due(
            base + DEBOUNCE - Duration::from_millis(1),
            Some(DEBOUNCE),
            MAX_WAIT
        ));
        assert!(window.is_due(base + DEBOUNCE, Some(DEBOUNCE), MAX_WAIT));
    }

    #[test]
    fn disabled_window_runs_immediately() {
        let base = Instant::now();
        let mut window = RefreshWindow::default();
        window.on_signal(base);

        assert_eq!(window.deadline(None, MAX_WAIT), Some(base));
        assert!(window.is_due(base, None, MAX_WAIT));
    }

    #[test]
    fn max_wait_never_shortens_the_window() {
        let base = Instant::now();
        let mut window = RefreshWindow::default();
        window.on_signal(base);
        window.on_signal(base + Duration::from_secs(20));

        // max_wait 比 debounce 还小：以 debounce 为准，避免窗口被配没
        assert_eq!(
            window.deadline(Some(DEBOUNCE), Duration::from_secs(1)),
            Some(base + DEBOUNCE)
        );
    }

    #[test]
    fn clear_resets_window() {
        let base = Instant::now();
        let mut window = RefreshWindow::default();
        window.on_signal(base);
        window.clear();

        assert_eq!(window.deadline(Some(DEBOUNCE), MAX_WAIT), None);
        assert!(!window.is_due(base + DEBOUNCE, Some(DEBOUNCE), MAX_WAIT));
    }
}
