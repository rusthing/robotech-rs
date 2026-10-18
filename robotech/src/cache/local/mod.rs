//! # 本地缓存
//!
//! 基于内存的本地缓存实现，通过消息队列在多节点间同步缓存变更。
//!
//! 当前实现：
//! - `nats`：基于 NATS JetStream 的本地缓存同步（需启用 feature `local-cache`）
//!
//! ## 与刷新信号的关系
//!
//! 订阅、探活、自愈这些通用能力在 [`crate::refresh`] 中实现，本模块只负责
//! 把「收到刷新信号」映射成「全量重建 + 原子替换快照」。
//!
//! 也就是说，缓存刷新只是刷新信号的一种用法：
//!
//! ```text
//! 写操作成功
//!   → refresh::publish(subject)
//!     → 各节点收到信号
//!       → refresh::subscribe 的 handler
//!         → rebuild_fn() 全量查库
//!         → ArcSwapOption::store 原子替换
//! ```
//!
//! 探活间隔由配置文件中的 `[refresh] probe-interval` 控制（默认 15 秒，
//! 历史段名 `[local-cache]` 仍兼容），见 [`crate::refresh::RefreshConfig`]。

mod event;
pub use event::*;

pub mod nats;

use std::sync::Arc;
use tracing::error;

pub use nats::publish_cache_refresh_signal;

/// # 安装本地缓存订阅
///
/// 订阅 `subject` 上的刷新信号，收到后调用 `rebuild_fn` 全量重建缓存并
/// 原子替换 `storage` 中的快照。
///
/// 底层由 [`crate::refresh::subscribe`] 提供，因此天然具备：
///
/// - **幂等**：同一 `subject` 重复调用只注册一次，配置热更新可安全重入；
/// - **自愈**：consumer 丢失后自动重建订阅，并立即补一次全量刷新；
/// - **不丢状态**：刷新信号本身允许丢，重建都是全量查库，事后补齐即可。
///
/// ## 参数
///
/// - `subject`：刷新信号主题
/// - `storage`：缓存快照的全局存储
/// - `rebuild_fn`：全量重建函数
pub async fn setup_local_cache<C, F, Fut>(
    subject: &str,
    storage: &'static arc_swap::ArcSwapOption<C>,
    rebuild_fn: F,
) -> anyhow::Result<Arc<tokio::task::JoinHandle<()>>>
where
    C: Send + Sync + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = anyhow::Result<C>> + Send + 'static,
{
    let rebuild = Arc::new(rebuild_fn);

    crate::refresh::subscribe(subject, move || {
        let rebuild = Arc::clone(&rebuild);
        async move {
            match (rebuild)().await {
                Ok(cache) => {
                    storage.store(Some(Arc::new(cache)));
                }
                // 失败已在本地记录，返回 Ok 避免刷新层再报一次同样的错误
                Err(e) => error!("缓存全量重建失败: {e}"),
            }
            Ok(())
        }
    })
    .await
}
