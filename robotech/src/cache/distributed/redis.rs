//! # Redis 分布式缓存
//!
//! 直接操作 Redis 的 CRUD 工具函数，用于分布式缓存的读写。
//! 复用 robotech 已有的 Redis 连接（[`crate::redis::get_redis_conn`]）。
//!
//! ## 特点
//!
//! - 无需同步层：Redis 本身是共享存储
//! - 支持 TTL 过期
//! - 基于 serde 自动序列化/反序列化
//!
//! ## 使用示例
//!
//! ```ignore
//! use robotech::cache::distributed::redis::RedisCache;
//! use std::time::Duration;
//!
//! // 写入
//! RedisCache::set("dtw:group:1", &group, Some(Duration::from_secs(3600))).await?;
//!
//! // 读取
//! let group: Option<MyType> = RedisCache::get("dtw:group:1").await?;
//!
//! // 删除
//! RedisCache::del("dtw:group:1").await?;
//! ```

use std::time::Duration;

use anyhow::Context;
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};

use crate::redis::get_redis_conn;

/// # Redis 缓存操作工具
///
/// 纯静态方法，无状态。所有方法共享同一 Redis 连接。
pub struct RedisCache;

impl RedisCache {
    /// # 读取键值
    ///
    /// ## 参数
    ///
    /// - `key`：Redis 键
    ///
    /// ## 返回值
    ///
    /// - `Some(T)`：键存在且反序列化成功
    /// - `None`：键不存在
    ///
    /// ## 错误
    ///
    /// 网络错误或反序列化失败时返回错误。
    pub async fn get<T: DeserializeOwned>(key: &str) -> anyhow::Result<Option<T>> {
        let mut conn = get_redis_conn()
            .map_err(|e| anyhow::anyhow!("Redis 连接失败: {e}"))?;
        let val: Option<String> = conn.get(key).await.with_context(|| format!("Redis GET {key} 失败"))?;
        match val {
            Some(s) => {
                let v = serde_json::from_str(&s)
                    .with_context(|| format!("Redis GET {key} 反序列化失败"))?;
                Ok(Some(v))
            }
            None => Ok(None),
        }
    }

    /// # 写入键值
    ///
    /// ## 参数
    ///
    /// - `key`：Redis 键
    /// - `val`：值引用，需实现 [`Serialize`]
    /// - `ttl`：过期时间。`None` 表示永不过期
    pub async fn set<T: Serialize>(key: &str, val: &T, ttl: Option<Duration>) -> anyhow::Result<()> {
        let mut conn = get_redis_conn()
            .map_err(|e| anyhow::anyhow!("Redis 连接失败: {e}"))?;
        let json = serde_json::to_string(val)
            .with_context(|| format!("Redis SET {key} 序列化失败"))?;
        match ttl {
            Some(t) => {
                let seconds = t.as_secs();
                if seconds > 0 {
                    let _: () = conn
                        .set_ex(key, json, seconds)
                        .await
                        .with_context(|| format!("Redis SETEX {key} 失败"))?;
                } else {
                    let _: () = conn
                        .set(key, json)
                        .await
                        .with_context(|| format!("Redis SET {key} 失败"))?;
                }
            }
            None => {
                let _: () = conn
                    .set(key, json)
                    .await
                    .with_context(|| format!("Redis SET {key} 失败"))?;
            }
        }
        Ok(())
    }

    /// # 删除键
    ///
    /// 返回被删除的键数量（0 表示键不存在）。
    pub async fn del(key: &str) -> anyhow::Result<usize> {
        let mut conn = get_redis_conn()
            .map_err(|e| anyhow::anyhow!("Redis 连接失败: {e}"))?;
        let count: usize = conn
            .del(key)
            .await
            .with_context(|| format!("Redis DEL {key} 失败"))?;
        Ok(count)
    }

    /// # 检查键是否存在
    pub async fn exists(key: &str) -> anyhow::Result<bool> {
        let mut conn = get_redis_conn()
            .map_err(|e| anyhow::anyhow!("Redis 连接失败: {e}"))?;
        let exists: bool = conn
            .exists(key)
            .await
            .with_context(|| format!("Redis EXISTS {key} 失败"))?;
        Ok(exists)
    }

    /// # 设置键过期时间
    ///
    /// 返回是否成功（键不存在时为 `false`）。
    pub async fn expire(key: &str, ttl: Duration) -> anyhow::Result<bool> {
        let mut conn = get_redis_conn()
            .map_err(|e| anyhow::anyhow!("Redis 连接失败: {e}"))?;
        let seconds = ttl.as_secs() as i64;
        let ok: bool = conn
            .expire(key, seconds)
            .await
            .with_context(|| format!("Redis EXPIRE {key} 失败"))?;
        Ok(ok)
    }
}