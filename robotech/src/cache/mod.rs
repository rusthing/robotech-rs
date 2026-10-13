//! # Cache 模块
//!
//! 提供多级缓存的统一抽象。
//!
//! ## 子模块
//!
//! - `local`：本地内存缓存，通过消息队列在多节点间同步
//! - `distributed`：分布式缓存，直接操作远程存储（如 Redis）
//!
//! ## Feature 开关
//!
//! - `local-cache`：启用本地缓存实现
//! - `distributed-cache`：启用分布式缓存实现

use arc_swap::ArcSwap;
use std::future::Future;
use std::sync::Arc;
use tokio::task::JoinHandle;

#[cfg(feature = "local-cache")]
pub mod local;
#[cfg(feature = "distributed-cache")]
pub mod distributed;

mod event;
pub use event::*;

// ═══════════════════════════════════════════════════════════════
//  统一 API（业务层只依赖这一层，不感知具体中间件）
// ═══════════════════════════════════════════════════════════════

/// # 发布缓存刷新信号
#[cfg(feature = "local-cache")]
pub use local::nats::publish_cache_refresh_signal;

/// # 启动本地缓存消费者
///
/// 根据编译时 feature 选择消息中间件，自动生成对应的 Stream/Consumer 名称。
/// 后续加 RabbitMQ 等，只需新增 feature + 实现，业务代码零改动。
#[cfg(feature = "local-cache")]
pub async fn setup_local_cache<C, F, Fut>(
    subject: &str,
    storage: &'static ArcSwap<C>,
    rebuild_fn: F,
) -> anyhow::Result<Arc<JoinHandle<()>>>
where
    C: Default + Send + Sync + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<C>> + Send + 'static,
{
    let stream_name = subject.replace('.', "_");
    let consumer_name = format!("{stream_name}_consumer");

    local::nats::setup_nats_cache(
        subject,
        local::nats::default_stream_config(&stream_name),
        local::nats::default_consumer_config(&consumer_name),
        storage,
        rebuild_fn,
    )
    .await
}