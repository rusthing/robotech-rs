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
//! - `cache-local-nats`：启用本地缓存 NATS 实现
//! - `cache-distributed-redis`：启用分布式缓存 Redis 实现

#[cfg(feature = "cache-local-nats")]
pub mod local;
#[cfg(feature = "cache-distributed-redis")]
pub mod distributed;

mod event;
pub use event::*;