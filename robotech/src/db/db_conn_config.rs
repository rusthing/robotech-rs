//! # 数据库配置模块
//!
//! 该模块定义了数据库连接相关的配置结构体和默认值

use sea_orm::ConnectOptions;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::time::Duration;
use wheel_rs::serde::{
    duration_option_option_serde, duration_option_serde, log_filter_option_serde,
};

/// # 数据库配置结构体
///
/// 用于存储数据库连接所需的各种配置参数
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct DbConnConfig {
    /// The URI of the database
    pub(crate) url: String,
    /// Maximum number of connections for a pool
    pub(crate) max_connections: Option<u32>,
    /// Minimum number of connections for a pool
    pub(crate) min_connections: Option<u32>,
    /// The connection timeout for a packet connection
    #[serde(default, with = "duration_option_serde")]                          // ← 加 default
    pub(crate) connect_timeout: Option<Duration>,
    /// Maximum idle time for a particular connection to prevent
    /// network resource exhaustion
    #[serde(default, with = "duration_option_option_serde")]                    // ← 加 default
    pub(crate) idle_timeout: Option<Option<Duration>>,
    /// Set the maximum amount of time to spend waiting for acquiring a connection
    #[serde(default, with = "duration_option_serde")]                          // ← 加 default
    pub(crate) acquire_timeout: Option<Duration>,
    /// Set the maximum lifetime of individual connections
    #[serde(default, with = "duration_option_option_serde")]                    // ← 加 default
    pub(crate) max_lifetime: Option<Option<Duration>>,
    /// Enable SQLx statement logging
    pub(crate) sqlx_logging: Option<bool>,
    /// Record SQL statements in tracing spans
    pub(crate) record_stmt_in_spans: Option<bool>,
    /// SQLx statement logging level (ignored if `sqlx_logging` is false)
    #[serde(default, with = "log_filter_option_serde")]                         // ← 加 default
    pub(crate) sqlx_logging_level: Option<log::LevelFilter>,
    /// SQLx slow statements logging level (ignored if `sqlx_logging` is false)
    #[serde(default, with = "log_filter_option_serde")]                         // ← 加 default
    pub(crate) sqlx_slow_statements_logging_level: Option<log::LevelFilter>,
    /// SQLx slow statements duration threshold (ignored if `sqlx_logging` is false)
    #[serde(default, with = "duration_option_serde")]                           // ← 加 default
    pub(crate) sqlx_slow_statements_logging_threshold: Option<Duration>,
    /// set sqlcipher key
    pub(crate) sqlcipher_key: Option<Cow<'static, str>>,
    /// Schema search path (PostgreSQL only)
    pub(crate) schema_search_path: Option<String>,
    /// Application name (PostgreSQL only)
    pub(crate) application_name: Option<String>,
    /// Statement timeout (PostgreSQL only)
    #[serde(default, with = "duration_option_serde")]                           // ← 加 default
    pub(crate) statement_timeout: Option<Duration>,
    pub(crate) test_before_acquire: Option<bool>,
    /// If set, a pooled connection is pinged before being handed out only when it has been
    /// idle for at least this long (see [`ConnectOptions::test_before_acquire_if_idle_for`]).
    pub(crate) test_before_acquire_if_idle_for: Option<Duration>,
    /// Only establish connections to the DB as needed. If set to `true`, the db connection will
    /// be created using SQLx's [connect_lazy](https://docs.rs/sqlx/latest/sqlx/struct.Pool.html#method.connect_lazy)
    /// method.
    pub(crate) connect_lazy: Option<bool>,
}

impl Into<ConnectOptions> for DbConnConfig {
    fn into(self) -> ConnectOptions {
        let mut opt = ConnectOptions::new(self.url);

        // 连接池
        if let Some(v) = self.max_connections {
            opt.max_connections(v);
        }
        if let Some(v) = self.min_connections {
            opt.min_connections(v);
        }
        if let Some(v) = self.connect_timeout {
            opt.connect_timeout(v);
        }
        if let Some(v) = self.idle_timeout {
            opt.idle_timeout(v);
        }
        if let Some(v) = self.acquire_timeout {
            opt.acquire_timeout(v);
        }
        if let Some(v) = self.max_lifetime {
            opt.max_lifetime(v);
        }

        // SQLx 日志
        if let Some(v) = self.sqlx_logging {
            opt.sqlx_logging(v);
        }
        if let Some(v) = self.record_stmt_in_spans {
            opt.record_stmt_in_spans(v);
        }
        if let Some(v) = self.sqlx_logging_level {
            opt.sqlx_logging_level(v);
        }

        // sqlx_slow_statements_logging_settings 是组合 setter，没有独立 setter
        // 任一项有值则两者一起设置，无值的用 SeaORM 默认值
        if self.sqlx_slow_statements_logging_level.is_some()
            || self.sqlx_slow_statements_logging_threshold.is_some()
        {
            let level = self
                .sqlx_slow_statements_logging_level
                .unwrap_or(log::LevelFilter::Off);
            let threshold = self
                .sqlx_slow_statements_logging_threshold
                .unwrap_or(Duration::from_secs(1));
            opt.sqlx_slow_statements_logging_settings(level, threshold);
        }

        // 扩展
        if let Some(v) = self.sqlcipher_key {
            opt.sqlcipher_key(v);
        }
        if let Some(v) = self.schema_search_path {
            opt.set_schema_search_path(v);
        }
        if let Some(v) = self.application_name {
            opt.set_application_name(v);
        }
        if let Some(v) = self.statement_timeout {
            opt.statement_timeout(v);
        }
        if let Some(v) = self.test_before_acquire {
            opt.test_before_acquire(v);
        }
        if let Some(v) = self.test_before_acquire_if_idle_for {
            opt.test_before_acquire_if_idle_for(v);
        }
        if let Some(v) = self.connect_lazy {
            opt.connect_lazy(v);
        }

        opt
    }
}

impl DbConnConfig {
    /// Get the database URL of the pool
    pub fn get_url(&self) -> &str {
        &self.url
    }
}
