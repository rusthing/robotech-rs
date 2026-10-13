//! # 本地缓存
//!
//! 基于内存的本地缓存实现，通过消息队列在多节点间同步缓存变更。
//!
//! 当前实现：
//! - `nats`：基于 NATS Pub/Sub 的本地缓存同步（需启用 feature `cache-local-nats`）

#[cfg(feature = "nats")]
pub mod nats;