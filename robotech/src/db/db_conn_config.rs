//! # 数据库配置模块
//!
//! 定义数据库连接配置结构体（`DbConnConfig`）及其到 SeaORM
//! [`ConnectOptions`](sea_orm::ConnectOptions) 的转换实现。

use sea_orm::ConnectOptions;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::time::Duration;
use wheel_rs::serde::{
    duration_option_option_serde, duration_option_serde, log_filter_option_serde,
};

/// # 数据库配置键
///
/// 配置文件中数据库配置段使用的键名，值为 `"db"`；
/// 同时用于判断数据库相关配置是否发生变更。
pub const DB_CONN_CONFIG_KEY: &str = "db";

/// # 数据库配置结构体
///
/// 用于存储数据库连接所需的各种配置参数。
///
/// 字段说明请参考 [`ConnectOptions`](sea_orm::ConnectOptions) 的对应方法。
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct DbConnConfig {
    /// 数据库连接 URI
    pub(crate) url: String,
    /// 连接池最大连接数
    pub(crate) max_connections: Option<u32>,
    /// 连接池最小连接数
    pub(crate) min_connections: Option<u32>,
    /// 连接超时时间
    #[serde(default, with = "duration_option_serde")]
    pub(crate) connect_timeout: Option<Duration>,
    /// 连接最大空闲时间（防止网络资源耗尽）
    #[serde(default, with = "duration_option_option_serde")]
    pub(crate) idle_timeout: Option<Option<Duration>>,
    /// 获取连接的最大等待时间
    #[serde(default, with = "duration_option_serde")]
    pub(crate) acquire_timeout: Option<Duration>,
    /// 单个连接的最大生命周期
    #[serde(default, with = "duration_option_option_serde")]
    pub(crate) max_lifetime: Option<Option<Duration>>,
    /// 是否启用 SQLx 语句日志
    pub(crate) sqlx_logging: Option<bool>,
    /// 是否在 tracing span 中记录 SQL 语句
    pub(crate) record_stmt_in_spans: Option<bool>,
    /// SQLx 语句日志级别（`sqlx_logging` 为 false 时忽略）
    #[serde(default, with = "log_filter_option_serde")]
    pub(crate) sqlx_logging_level: Option<log::LevelFilter>,
    /// SQLx 慢语句日志级别（`sqlx_logging` 为 false 时忽略）
    #[serde(default, with = "log_filter_option_serde")]
    pub(crate) sqlx_slow_statements_logging_level: Option<log::LevelFilter>,
    /// SQLx 慢语句阈值（`sqlx_logging` 为 false 时忽略）
    #[serde(default, with = "duration_option_serde")]
    pub(crate) sqlx_slow_statements_logging_threshold: Option<Duration>,
    /// SQLCipher 密钥
    pub(crate) sqlcipher_key: Option<Cow<'static, str>>,
    /// Schema 搜索路径（仅 PostgreSQL）
    pub(crate) schema_search_path: Option<String>,
    /// 应用名称（仅 PostgreSQL）
    pub(crate) application_name: Option<String>,
    /// 语句超时时间（仅 PostgreSQL）
    #[serde(default, with = "duration_option_serde")]
    pub(crate) statement_timeout: Option<Duration>,
    /// 获取连接前是否先 ping 测试
    pub(crate) test_before_acquire: Option<bool>,
    /// 仅当连接空闲时间达到此阈值时才 ping 测试（见 [`ConnectOptions::test_before_acquire_if_idle_for`]）
    pub(crate) test_before_acquire_if_idle_for: Option<Duration>,
    /// 是否按需建立连接（使用 SQLx 的 `connect_lazy` 方法）
    pub(crate) connect_lazy: Option<bool>,
}

/// 将 `DbConnConfig` 转换为 SeaORM 的 [`ConnectOptions`]，
/// 仅设置有值的字段，无值字段使用 SeaORM 默认值。
impl From<DbConnConfig> for ConnectOptions {
    fn from(config: DbConnConfig) -> ConnectOptions {
        let mut opt = ConnectOptions::new(config.url);

        // 连接池
        if let Some(v) = config.max_connections {
            opt.max_connections(v);
        }
        if let Some(v) = config.min_connections {
            opt.min_connections(v);
        }
        if let Some(v) = config.connect_timeout {
            opt.connect_timeout(v);
        }
        if let Some(v) = config.idle_timeout {
            opt.idle_timeout(v);
        }
        if let Some(v) = config.acquire_timeout {
            opt.acquire_timeout(v);
        }
        if let Some(v) = config.max_lifetime {
            opt.max_lifetime(v);
        }

        // SQLx 日志
        if let Some(v) = config.sqlx_logging {
            opt.sqlx_logging(v);
        }
        if let Some(v) = config.record_stmt_in_spans {
            opt.record_stmt_in_spans(v);
        }
        if let Some(v) = config.sqlx_logging_level {
            opt.sqlx_logging_level(v);
        }

        // sqlx_slow_statements_logging_settings 是组合 setter，没有独立 setter
        // 任一项有值则两者一起设置，无值的用 SeaORM 默认值
        if config.sqlx_slow_statements_logging_level.is_some()
            || config.sqlx_slow_statements_logging_threshold.is_some()
        {
            let level = config
                .sqlx_slow_statements_logging_level
                .unwrap_or(log::LevelFilter::Off);
            let threshold = config
                .sqlx_slow_statements_logging_threshold
                .unwrap_or(Duration::from_secs(1));
            opt.sqlx_slow_statements_logging_settings(level, threshold);
        }

        // 扩展
        if let Some(v) = config.sqlcipher_key {
            opt.sqlcipher_key(v);
        }
        if let Some(v) = config.schema_search_path {
            opt.set_schema_search_path(v);
        }
        if let Some(v) = config.application_name {
            opt.set_application_name(v);
        }
        if let Some(v) = config.statement_timeout {
            opt.statement_timeout(v);
        }
        if let Some(v) = config.test_before_acquire {
            opt.test_before_acquire(v);
        }
        if let Some(v) = config.test_before_acquire_if_idle_for {
            opt.test_before_acquire_if_idle_for(v);
        }
        if let Some(v) = config.connect_lazy {
            opt.connect_lazy(v);
        }

        opt
    }
}

impl DbConnConfig {
    /// # 获取数据库连接 URL
    ///
    /// 返回当前配置的数据库连接字符串。
    pub fn get_url(&self) -> &str {
        &self.url
    }
}
