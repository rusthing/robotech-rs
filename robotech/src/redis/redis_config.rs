use arc_swap::ArcSwapOption;
use config::Value;
use redis::aio::MultiplexedConnection;
use redis::{Client, RedisError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;
use wheel_rs::config_utils::has_config_changed;

/// Redis 配置键
pub const REDIS_CONFIG_KEY: &str = "redis";

static REDIS_CONN: ArcSwapOption<MultiplexedConnection> = ArcSwapOption::const_empty();

pub fn get_redis_conn() -> Result<MultiplexedConnection, RedisError> {
    REDIS_CONN
        .load_full()
        .map(|arc| arc.as_ref().clone())
        .ok_or_else(|| RedisError::from((redis::ErrorKind::Io, "Redis 连接未初始化")))
}

pub async fn setup_redis_conn(
    config: RedisConfig,
    changed: &Option<HashMap<String, Value>>,
) -> Result<(), RedisError> {
    info!("setup redis connection...: {config:?}");
    if changed
        .as_ref()
        .map(|changed| has_config_changed(REDIS_CONFIG_KEY, changed))
        .unwrap_or(true)
    {
        let client = Client::open(config.url.as_str())?;
        let conn = client.get_multiplexed_async_connection().await?;
        REDIS_CONN.store(Some(Arc::new(conn)));
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct RedisConfig {
    /// Redis 连接地址，例如 `redis://127.0.0.1:6379`
    pub url: String,
}