//! # 缓存变更事件
//!
//! 缓存刷新信号，用于 JetStream 持久化广播。
//! 收到信号后，所有节点执行全量刷新（从 DB 或上游服务重建缓存）。

use serde::{Deserialize, Serialize};

/// # 缓存刷新信号
///
/// 所有写操作（增删改）统一发布此信号通知其他节点刷新缓存。
/// 不使用 Add/Modify/Del 细分事件，全量刷新更简单可靠。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheRefreshSignal {
    pub action: &'static str,
}

impl CacheRefreshSignal {
    pub fn new() -> Self {
        Self {
            action: "refresh",
        }
    }
}

impl Default for CacheRefreshSignal {
    fn default() -> Self {
        Self::new()
    }
}