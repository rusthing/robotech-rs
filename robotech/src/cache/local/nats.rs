//! # NATS 本地缓存同步
//!
//! 这里是 [`crate::refresh`] 在缓存场景下的一层薄封装：
//!
//! - 刷新信号、订阅自愈、探活逻辑都在 `refresh::nats` 中实现
//! - 本模块只负责把「收到信号」映射成「重建缓存 + 原子替换」
//!
//! ## 使用示例
//!
//! ```ignore
//! use arc_swap::ArcSwapOption;
//! use robotech::cache::local::nats;
//!
//! static CACHE: ArcSwapOption<DtwCache> = ArcSwapOption::const_empty();
//!
//! nats::setup_nats_cache(
//!     "dtw.cache.refresh",
//!     nats::default_stream_config("dtw.cache.refresh"),
//!     nats::default_consumer_config("dtw_cache_refresh_consumer"),
//!     &CACHE,
//!     || async { build_cache_from_db().await },
//! ).await?;
//! ```
//!
//! 推荐直接使用 [`crate::refresh::subscribe`]，它还会做幂等注册与自愈。

use arc_swap::ArcSwapOption;
use async_nats::jetstream;
use std::future::Future;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info};

pub use crate::refresh::nats::{
    consumer_alive, default_consumer_config, default_stream_config,
};

/// # 启动 NATS 缓存消费者
///
/// 创建 JetStream Push Consumer，收到刷新信号后调用 `rebuild_fn` 重建缓存
/// 并通过 `storage` 原子替换。
///
/// > 新代码建议直接用 [`crate::refresh::subscribe`]，自行组合「重建 + 存储」；
/// > 本函数保留是为了兼容既有调用方。
///
/// ## 参数
///
/// - `subject`：JetStream Subject
/// - `stream_config`：Stream 配置（[`default_stream_config`] 提供合理默认值）
/// - `consumer_config`：Push Consumer 配置（[`default_consumer_config`] 提供合理默认值）
/// - `storage`：`ArcSwapOption<C>` 静态引用，读端通过 `load_full()` 获取快照
/// - `rebuild_fn`：异步重建函数，返回新的缓存数据
pub async fn setup_nats_cache<C, F, Fut>(
    subject: &str,
    stream_config: jetstream::stream::Config,
    consumer_config: jetstream::consumer::push::Config,
    storage: &'static ArcSwapOption<C>,
    rebuild_fn: F,
) -> anyhow::Result<Arc<JoinHandle<()>>>
where
    C: Send + Sync + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<C>> + Send + 'static,
{
    let rebuild_fn = Arc::new(rebuild_fn);

    crate::refresh::nats::install(
        subject,
        stream_config,
        consumer_config,
        move || {
            let rebuild_fn = Arc::clone(&rebuild_fn);
            async move {
                match rebuild_fn().await {
                    Ok(new_cache) => {
                        storage.store(Some(Arc::new(new_cache)));
                        info!("缓存刷新完成");
                    }
                    Err(e) => error!("缓存全量重建失败: {e}"),
                }
                Ok(())
            }
        },
    )
    .await
}

/// # 发布缓存刷新信号
///
/// 通过 JetStream 持久化发布，所有订阅该 subject 的节点收到后执行全量重建。
///
/// 用于 SVC `after_*` 回调：任何写操作成功后调用此函数通知其他节点。
pub async fn publish_cache_refresh_signal(subject: &str) -> anyhow::Result<()> {
    crate::refresh::publish(subject).await
}
