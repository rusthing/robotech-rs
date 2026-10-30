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

    /// # 仅在键不存在时写入，并同时带上过期时间（SET NX EX）
    ///
    /// ## 为什么必须是原子的
    ///
    /// 「先 `exists` 再 `set`」会在两个并发请求之间留一个窗口：两边都判「不
    /// 存在」、都写入，去重就失效了。典型用法是「同一个对象的处理窗口」——
    /// 窗口要拦的恰恰就是并发的那几条。
    ///
    /// ## 为什么用 Lua 而不是 `SETNX` + `EXPIRE`
    ///
    /// 两步之间有个致命中间态：`SETNX` 成功但 `EXPIRE` 没发出去（连接断、
    /// 进程崩），这个键就**永不过期**——去重从此变成「这个对象只处理一次」，
    /// 后面的全被吞掉。对报警这类「宁可重复、不可丢」的业务，这是不可接受的
    /// 失败模式。放进一段 Lua 里，Redis 保证 SET 与 EX 同时生效。
    ///
    /// ## 参数
    ///
    /// - `key`：Redis 键
    /// - `val`：值引用，需实现 [`Serialize`]
    /// - `ttl`：过期时间，至少 1 秒（`EX` 的单位是秒，传 0 会被 Redis 拒绝）
    ///
    /// ## 返回值
    ///
    /// - `Ok(true)`：本次抢到（键原本不存在，已写入并带上 TTL）
    /// - `Ok(false)`：键已存在，本次没抢到
    pub async fn set_nx<T: Serialize>(key: &str, val: &T, ttl: Duration) -> anyhow::Result<bool> {
        // 秒级 TTL 是 Redis EX 的最小粒度；不足 1 秒按 1 秒算，好过直接报错
        const SET_NX_EX: &str = r"
            if redis.call('SET', KEYS[1], ARGV[1], 'NX', 'EX', ARGV[2]) then
                return 1
            end
            return 0
        ";

        let mut conn = get_redis_conn()
            .map_err(|e| anyhow::anyhow!("Redis 连接失败: {e}"))?;
        let json = serde_json::to_string(val)
            .with_context(|| format!("Redis SETNX {key} 序列化失败"))?;
        let seconds = ttl.as_secs().max(1);

        let acquired: i32 = redis::Script::new(SET_NX_EX)
            .key(key)
            .arg(json)
            .arg(seconds)
            .invoke_async(&mut conn)
            .await
            .with_context(|| format!("Redis SET NX EX {key} 失败"))?;

        Ok(acquired == 1)
    }
}