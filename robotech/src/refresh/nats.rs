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

use async_nats::jetstream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::mq::nats::{get_or_create_stream, publish as nats_publish, subscribe as nats_subscribe};

use super::config::get_refresh_config;

/// 已注册的订阅：subject → 守护任务句柄
///
/// 用于保证同一主题只注册一次——配置热更新会重新执行 `setup()`，
/// 业务代码重复调用 [`subscribe`] 时不能叠加订阅和守护任务。
static REGISTERED: LazyLock<Mutex<HashMap<String, Arc<JoinHandle<()>>>>> =
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
/// 收到信号后执行 `handler`；consumer 丢失时自动重建订阅并立即执行一次
/// `handler`，把中断期间漏掉的变更补齐。
///
/// 同一 `subject` 重复调用只会注册一次，返回同一个守护任务句柄。
///
/// ## 参数
///
/// - `subject`：刷新信号主题
/// - `handler`：收到信号后执行的异步动作，内部应做「全量重建」而不是增量更新
///
/// ## 返回值
///
/// 返回守护任务句柄；`abort()` 会同时停掉订阅与探活。
pub async fn subscribe<H, Fut>(subject: &str, handler: H) -> anyhow::Result<Arc<JoinHandle<()>>>
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
    registered.insert(subject.to_string(), Arc::clone(&watchdog));

    Ok(watchdog)
}

/// # 发布刷新信号
///
/// 通过 JetStream 持久化发布，所有订阅该主题的节点收到后各自执行刷新动作。
///
/// 用于 SVC 写操作成功后的 `after_write` 回调。
pub async fn publish(subject: &str) -> anyhow::Result<()> {
    nats_publish(subject, &RefreshSignal::new(), true).await?;
    Ok(())
}

/// # 安装订阅与守护（不幂等）
///
/// 供 [`subscribe`] 与缓存层的兼容入口使用。
pub(crate) async fn install<H, Fut>(
    subject: &str,
    stream_config: jetstream::stream::Config,
    consumer_config: jetstream::consumer::push::Config,
    handler: H,
) -> anyhow::Result<Arc<JoinHandle<()>>>
where
    H: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let handler = Arc::new(handler);
    let handle = open_subscription(
        subject,
        stream_config.clone(),
        consumer_config.clone(),
        Arc::clone(&handler),
    )
    .await?;

    Ok(Arc::new(spawn_watchdog(
        subject.to_string(),
        stream_config,
        consumer_config,
        handler,
        handle,
    )))
}

/// # 建立一次订阅
///
/// 先清理同名 consumer，保证新建的 consumer 不重放历史信号（启动与重连都走这里）。
async fn open_subscription<H, Fut>(
    subject: &str,
    stream_config: jetstream::stream::Config,
    consumer_config: jetstream::consumer::push::Config,
    handler: Arc<H>,
) -> anyhow::Result<Arc<JoinHandle<()>>>
where
    H: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    if let Some(consumer_name) = consumer_config.name.as_ref() {
        cleanup_stale_consumer(&stream_config, consumer_name).await;
    }

    let handle = nats_subscribe(
        subject,
        None,
        Some(stream_config),
        Some(consumer_config),
        move |_msg| {
            let handler = Arc::clone(&handler);
            async move {
                // 处理失败只记日志：动作是全量重建，下一次信号或守护补刷会覆盖；
                // 返回 Err 会让 JetStream 无限重投同一条无内容的信号。
                if let Err(e) = handler().await {
                    error!("处理刷新信号失败: {e}");
                }
                Ok(())
            }
        },
    )
    .await?;

    info!("刷新订阅已建立 (subject={subject})");
    Ok(handle)
}

/// # 订阅守护任务
///
/// 周期性探活，consumer 丢失则重建订阅并立即执行一次 `handler`。
fn spawn_watchdog<H, Fut>(
    subject: String,
    stream_config: jetstream::stream::Config,
    consumer_config: jetstream::consumer::push::Config,
    handler: Arc<H>,
    initial_handle: Arc<JoinHandle<()>>,
) -> JoinHandle<()>
where
    H: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let consumer_name = consumer_config.name.clone().unwrap_or_default();

    tokio::spawn(async move {
        let mut handle = initial_handle;

        loop {
            // 每轮取一次，配置热更新后下一轮即生效
            let probe_interval = get_refresh_config().effective_probe_interval();
            tokio::time::sleep(probe_interval).await;

            match consumer_alive(&stream_config, &consumer_name).await {
                Ok(true) => continue,
                Ok(false) => {
                    warn!("刷新消费者 {consumer_name} 已丢失，重建订阅");
                }
                Err(e) => {
                    warn!("刷新消费者探活失败，稍后重试: {e}");
                    continue;
                }
            }

            handle.abort();

            match open_subscription(
                &subject,
                stream_config.clone(),
                consumer_config.clone(),
                Arc::clone(&handler),
            )
            .await
            {
                Ok(new_handle) => {
                    handle = new_handle;
                    info!("刷新订阅已重建 (consumer={consumer_name})");

                    // 中断期间发出的信号已经丢了，直接补一次刷新
                    if let Err(e) = handler().await {
                        error!("重建后补刷新失败: {e}");
                    }
                }
                Err(e) => error!("重建刷新订阅失败，下一轮重试: {e}"),
            }
        }
    })
}

/// # 默认 Stream 配置
///
/// - `subject`：刷新信号主题，Stream 名由主题派生（`.` 替换为 `_`）
/// - `subjects` 精确匹配该主题，保证与发布主题一致
/// - 文件持久化，7 天 TTL
pub fn default_stream_config(subject: &str) -> jetstream::stream::Config {
    jetstream::stream::Config {
        name: subject.replace('.', "_"),
        subjects: vec![subject.to_string()],
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
