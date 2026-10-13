//! # 本地缓存
//!
//! 基于内存的本地缓存实现，通过消息队列在多节点间同步缓存变更。
//!
//! 当前实现：
//! - `nats`：基于 NATS Pub/Sub 的本地缓存同步（需启用 feature `local-cache`）

mod event;
pub use event::*;

pub mod nats;

// ═══════════════════════════════════════════════════════════════
//  统一 API
// ═══════════════════════════════════════════════════════════════

pub use nats::publish_cache_refresh_signal;

pub async fn setup_local_cache<C, F, Fut>(
    subject: &str,
    storage: &'static arc_swap::ArcSwap<C>,
    rebuild_fn: F,
) -> anyhow::Result<std::sync::Arc<tokio::task::JoinHandle<()>>>
where
    C: Default + Send + Sync + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = anyhow::Result<C>> + Send + 'static,
{
    let stream_name = subject.replace('.', "_");
    let consumer_name = format!("{stream_name}_consumer");

    nats::setup_nats_cache(
        subject,
        nats::default_stream_config(&stream_name),
        nats::default_consumer_config(&consumer_name),
        storage,
        rebuild_fn,
    )
    .await
}
