//! # 数据库配置模块
//!
//! 该模块定义了数据库连接相关的配置结构体和默认值

use log::LevelFilter;
use serde::{Deserialize, Serialize};
use wheel_rs::serde::log_filter_option_serde;

/// # 数据库配置结构体
///
/// 用于存储数据库连接所需的各种配置参数
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct DbConfig {
    /// 数据库连接URL
    ///
    /// 例如: postgres://user:password@localhost/database
    #[serde(default = "url_default")]
    pub url: String,

    /// 日志输出级别配置
    ///
    /// 控制数据库相关操作的日志输出级别
    #[serde(with = "log_filter_option_serde", default = "log_level_default")]
    pub log_level: Option<LevelFilter>,
}

impl Default for DbConfig {
    fn default() -> Self {
        db_default()
    }
}

/// # 数据库URL默认值
///
/// 返回空字符串作为默认的数据库URL
fn url_default() -> String {
    "".to_string()
}

/// # 日志输出级别默认值
///
/// 返回 [LevelFilter::Trace] 作为默认的日志级别
fn log_level_default() -> Option<LevelFilter> {
    Some(LevelFilter::Trace)
}

/// # 数据库配置默认值
///
/// 创建并返回一个具有默认值的 [DbConfig] 实例
fn db_default() -> DbConfig {
    DbConfig {
        url: url_default(),
        log_level: log_level_default(),
    }
}
