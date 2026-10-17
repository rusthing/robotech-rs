//! # 本地缓存
//!
//! 基于内存的本地缓存实现，通过消息队列在多节点间同步缓存变更。
//!
//! 当前实现：
//! - `nats`：基于 NATS Pub/Sub 的本地缓存同步（需启用 feature `local-cache`）
//!
//! ## 订阅自愈
//!
//! 刷新信号本身可以丢：任何节点重建缓存都是全量查库，只要事后补一次全量
//! 刷新，状态就重新对齐。所以这里不追求「消息不丢」，而是保证「订阅断了
//! 能被发现并补齐」。
//!
//! [`setup_local_cache`] 建立订阅后会启动一个守护任务，周期性探活：
//!
//! - consumer 还在：什么都不做（正常空闲时消息会自动补投）；
//! - consumer 不在了（NATS 重启、断连超过 `inactive_threshold`、被人工删除）：
//!   abort 旧订阅 → 重建订阅 → 立即全量刷新一次，补齐中断期间漏掉的变更；
//! - 探活本身失败（NATS 不可达）：保持现状，下一轮再试，避免抖动触发重建。

mod event;
pub use event::*;

pub mod nats;

use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

pub use nats::publish_cache_refresh_signal;

/// 订阅守护的探活间隔
const CONSUMER_PROBE_INTERVAL: Duration = Duration::from_secs(15);

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
    let stream_config = nats::default_stream_config(subject);
    let stream_name = stream_config.name.clone();

    // Push Consumer 按节点隔离：consumer 名固定会让多副本共用同一个 consumer，
    // 各节点启动时清理「自己的」consumer 就会把其它节点正在使用的 consumer 删掉。
    // 节点标识直接用全局实例 ID（ULID 或 `{app_name}-{ip}-{port}`，均为合法字符）。
    let consumer_name = format!("{stream_name}_consumer_{}", crate::env::get_app_instance_id());
    let consumer_config = nats::default_consumer_config(&consumer_name);

    // 订阅与守护都要拿到 rebuild_fn，用 Arc 共享
    let rebuild = Arc::new(rebuild_fn);

    let handle = nats::setup_nats_cache(
        subject,
        stream_config.clone(),
        consumer_config.clone(),
        storage,
        {
            let rebuild = Arc::clone(&rebuild);
            move || rebuild()
        },
    )
    .await?;

    let subject = subject.to_string();
    let watchdog = tokio::spawn(async move {
        let mut handle = handle;

        loop {
            tokio::time::sleep(CONSUMER_PROBE_INTERVAL).await;

            match nats::consumer_alive(&stream_config, &consumer_name).await {
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

            match nats::setup_nats_cache(
                &subject,
                stream_config.clone(),
                consumer_config.clone(),
                storage,
                {
                    let rebuild = Arc::clone(&rebuild);
                    move || rebuild()
                },
            )
            .await
            {
                Ok(new_handle) => {
                    handle = new_handle;
                    info!("刷新订阅已重建 (consumer={consumer_name})");

                    // 中断期间发出的刷新信号已经丢了，直接补一次全量刷新
                    match (rebuild)().await {
                        Ok(cache) => {
                            storage.store(Some(Arc::new(cache)));
                            info!("重建后全量刷新完成");
                        }
                        Err(e) => error!("重建后全量刷新失败: {e}"),
                    }
                }
                Err(e) => error!("重建刷新订阅失败，下一轮重试: {e}"),
            }
        }
    });

    Ok(Arc::new(watchdog))
}
