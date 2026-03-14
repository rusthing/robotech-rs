//! # 数据库配置模块
//!
//! 该模块定义了数据库连接相关的配置结构体和默认值

use log::LevelFilter;
use serde::{Deserialize, Serialize};
use wheel_rs::serde::log_filter_serde;

/// # 数据库配置结构体
///
/// 用于存储数据库连接所需的各种配置参数
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct DbConnConfig {
    /// 数据库连接URL
    ///
    /// 例如: postgres://user:password@localhost/database
    #[serde(default)]
    pub url: String,

    /// 日志输出级别配置
    ///
    /// 控制数据库相关操作的日志输出级别
    #[serde(with = "log_filter_serde", default = "log_level_default")]
    pub log_level: LevelFilter,
}

impl Default for DbConnConfig {
    fn default() -> Self {
        Self {
            url: String::default(),
            log_level: log_level_default(),
        }
    }
}

/// # 日志输出级别默认值
///
/// 返回 [LevelFilter::Debug] 作为默认的日志级别
fn log_level_default() -> LevelFilter {
    LevelFilter::Debug
}
