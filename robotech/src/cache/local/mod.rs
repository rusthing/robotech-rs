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
    storage: &'static arc_swap::ArcSwapOption<C>,
    rebuild_fn: F,
) -> anyhow::Result<std::sync::Arc<tokio::task::JoinHandle<()>>>
where
    C: Send + Sync + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = anyhow::Result<C>> + Send + 'static,
{
    let stream_config = nats::default_stream_config(subject);
    let stream_name = stream_config.name.clone();

    // Push Consumer 按节点隔离：consumer 名固定会让多副本共用同一个 consumer，
    // 各节点启动时清理「自己的」consumer 就会把其它节点正在使用的 consumer 删掉。
    // 节点标识直接用全局实例 ID（ULID 或 `{app_name}-{ip}-{port}`，均为合法字符）。
    let consumer_name = format!("{stream_name}_consumer_{}", crate::env::get_app_instance_id());

    nats::setup_nats_cache(
        subject,
        stream_config,
        nats::default_consumer_config(&consumer_name),
        storage,
        rebuild_fn,
    )
    .await
}