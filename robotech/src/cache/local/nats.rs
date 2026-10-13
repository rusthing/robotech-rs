//! # NATS 本地缓存
//!
//! 基于 `ArcSwap` 的无锁本地内存缓存 + JetStream 持久化刷新信号。
//!
//! ## 工作原理
//!
//! 1. **存储**：`ArcSwap<C>` 无锁并发读
//! 2. **同步**：任何节点的写操作通过 JetStream 发布刷新信号，所有节点收到后执行全量重建
//! 3. **启动**：应用启动时直接查询 DB 全量加载，不从 JetStream 重放历史
//! 4. **断网恢复**：durable consumer 在 NATS 服务端保留进度，重连后自动接续
//!
//! ## 与旧版差异
//!
//! - ❌ 不再细分 Add/Modify/Del 事件，统一为刷新信号
//! - ❌ 不再使用 `RwLock<HashMap<TypeId, ...>>`，改用业务侧持有的独立 `ArcSwap`
//! - ❌ 不再使用 Core NATS，改用 JetStream（`publish(persistent=true)` + Push Consumer）
//! - ✅ 消除精确增量更新的复杂 trait 和全量克隆开销
//! - ✅ 真正无锁读（`ArcSwap::load_full()`）
//!
//! ## 使用示例
//!
//! ```ignore
//! use arc_swap::ArcSwap;
//! use robotech::cache::local::nats;
//! use std::sync::Arc;
//!
//! static CACHE: ArcSwap<DtwCache> = ArcSwap::const_empty();
//!
//! // 启动 JetStream 消费者
//! nats::setup_nats_cache(
//!     "dtw.cache.refresh",
//!     nats::default_stream_config("dtw-cache"),
//!     nats::default_consumer_config("dtw-cache-consumer"),
//!     &CACHE,
//!     || async { build_cache_from_db().await },
//! ).await?;
//!
//! // 启动时全量加载
//! CACHE.store(Arc::new(build_cache_from_db().await));
//!
//! // 读：零锁开销
//! let snapshot = CACHE.load_full();
//!
//! // 发布刷新信号
//! nats::publish_cache_refresh_signal("dtw.cache.refresh").await?;
//! ```

use arc_swap::ArcSwap;
use async_nats::jetstream;
use std::future::Future;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::cache::CacheRefreshSignal;
use crate::mq::nats::{
    get_or_create_stream, publish as nats_publish,
    subscribe as nats_subscribe,
};

/// # 启动 NATS 缓存消费者
///
/// 创建 JetStream Push Consumer，收到刷新信号后调用 `rebuild_fn` 重建缓存
/// 并通过 `storage` 原子替换。
///
/// ## 参数
///
/// - `subject`：JetStream Subject
/// - `stream_config`：Stream 配置（[`default_stream_config`] 提供合理默认值）
/// - `consumer_config`：Push Consumer 配置（[`default_consumer_config`] 提供合理默认值，
///   内部会设置 `DeliverPolicy::New` 确保新建消费者不重放历史）
/// - `storage`：`ArcSwap<C>` 静态引用，读端通过 `load_full()` 获取快照
/// - `rebuild_fn`：异步重建函数，返回新的缓存数据
///
/// ## 返回值
///
/// 成功返回订阅句柄，可调用 `.abort()` 停止消费。
pub async fn setup_nats_cache<C, F, Fut>(
    subject: &str,
    stream_config: jetstream::stream::Config,
    consumer_config: jetstream::consumer::push::Config,
    storage: &'static ArcSwap<C>,
    rebuild_fn: F,
) -> anyhow::Result<Arc<JoinHandle<()>>>
where
    C: Default + Send + Sync + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<C>> + Send + 'static,
{
    let rebuild_fn = Arc::new(rebuild_fn);

    // 启动时删除旧 consumer，避免重放历史消息。
    // 全量刷新由 main.rs 在 subscribe 之后显式调用。
    if let Some(ref consumer_name) = consumer_config.name {
        cleanup_stale_consumer(&stream_config, consumer_name).await;
    }

    let handle = nats_subscribe(
        subject,
        None,
        Some(stream_config),
        Some(consumer_config),
        move |_msg| {
            let rebuild_fn = Arc::clone(&rebuild_fn);
            async move {
                match rebuild_fn().await {
                    Ok(new_cache) => {
                        storage.store(Arc::new(new_cache));
                        info!("缓存刷新完成");
                        Ok(())
                    }
                    Err(e) => {
                        error!("缓存全量重建失败: {e}");
                        Ok(())
                    }
                }
            }
        },
    )
    .await?;

    info!("NATS 缓存消费者启动完成 (subject={subject})");
    Ok(handle)
}

/// # 发布缓存刷新信号
///
/// 通过 JetStream 持久化发布，所有订阅该 subject 的节点收到后执行全量重建。
///
/// 用于 SVC `after_*` 回调：任何写操作成功后调用此函数通知其他节点。
pub async fn publish_cache_refresh_signal(subject: &str) -> anyhow::Result<()> {
    let signal = CacheRefreshSignal::new();
    nats_publish(subject, &signal, true).await?;
    Ok(())
}

/// # 默认 Stream 配置
///
/// - `subjects` 包含指定的 subject（`>` 通配符版本以便扩展）
/// - 文件持久化，无消息数/大小限制（由后续 TTL 控制）
/// - 每节点独立 consumer，不做负载均衡
pub fn default_stream_config(stream_name: &str) -> jetstream::stream::Config {
    jetstream::stream::Config {
        name: stream_name.to_string(),
        subjects: vec![format!("{stream_name}.>")],
        storage: jetstream::stream::StorageType::File,
        max_messages: -1,
        max_bytes: -1,
        max_age: std::time::Duration::from_secs(604800), // 7 天 TTL
        ..Default::default()
    }
}

/// # 默认 Push Consumer 配置
///
/// - `DeliverPolicy::New`：新建消费者不重放历史消息
/// - `AckPolicy::Explicit`：处理完成后 ACK
/// - `name` 设置为 consumer_name，确保 durable（断网重连保障）
pub fn default_consumer_config(consumer_name: &str) -> jetstream::consumer::push::Config {
    jetstream::consumer::push::Config {
        name: Some(consumer_name.to_string()),
        deliver_policy: jetstream::consumer::DeliverPolicy::New,
        ack_policy: jetstream::consumer::AckPolicy::Explicit,
        ..Default::default()
    }
}

/// # 清理旧的 Push Consumer
///
/// 启动时删除同名 consumer，确保不重放历史消息。
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