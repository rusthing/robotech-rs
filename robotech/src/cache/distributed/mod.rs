//! # 分布式缓存
//!
//! 基于远程存储的分布式缓存实现，直接操作远程存储，无需节点间同步。
//!
//! 当前实现：
//! - `redis`：基于 Redis 的分布式缓存（需启用 feature `distributed-cache`）

#[cfg(feature = "redis")]
pub mod redis;