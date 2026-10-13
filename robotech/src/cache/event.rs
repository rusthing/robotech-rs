//! # 缓存变更事件
//!
//! 通用的 Add/Modify/Del 事件结构，用于本地缓存的同步消息。

use serde::{Deserialize, Serialize};

/// # 缓存变更事件
///
/// 使用 `#[serde(tag = "action")]` 按 action 字段区分操作类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "data")]
pub enum CacheEvent {
    /// 新增实体，data 为实体的 JSON 表示
    Add(serde_json::Value),
    /// 修改实体，data 为修改后的实体 JSON 表示
    Modify(serde_json::Value),
    /// 删除实体，data 为实体 ID（u64）
    Del(u64),
}