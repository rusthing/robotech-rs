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

#[cfg(feature = "distributed-cache")]
pub mod distributed;
#[cfg(feature = "local-cache")]
mod local;

#[cfg(feature = "distributed-cache")]
pub use distributed::*;

#[cfg(feature = "local-cache")]
pub use local::*;
