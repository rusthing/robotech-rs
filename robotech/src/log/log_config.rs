use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing_appender::rolling::Rotation;
use wheel_rs::serde::rotation_serde;

/// # 日志配置
///
/// 日志初始化与热更新所需的配置项，支持从配置文件中反序列化（kebab-case 命名）。
///
/// 字段说明：
/// - `level`：全局日志级别
/// - `modules`：按模块（target）覆盖的日志级别
/// - `console_time_format`：控制台日志时间格式
/// - `file_time_format`：文件日志时间格式
/// - `rotation`：日志文件滚动策略
/// - `show_spans`：是否在控制台输出中打印 span 链
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LogConfig {
    /// 日志级别
    #[serde(default = "level_default")]
    pub level: String,
    /// 模块日志级别
    #[serde(default)]
    pub modules: HashMap<String, String>,
    /// 控制台时间格式
    #[serde(default = "console_time_format_default")]
    pub console_time_format: String,
    /// 文件时间格式
    #[serde(default = "file_time_format_default")]
    pub file_time_format: String,
    /// 日志文件滚动策略
    #[serde(with = "rotation_serde", default = "log_rotation_default")]
    pub rotation: Rotation,
    /// 是否显示spans
    #[serde(default)]
    pub show_spans: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: level_default(),
            modules: HashMap::default(),
            console_time_format: console_time_format_default(),
            file_time_format: file_time_format_default(),
            rotation: log_rotation_default(),
            show_spans: bool::default(),
        }
    }
}

fn level_default() -> String {
    "info".to_string()
}

fn console_time_format_default() -> String {
    "%H:%M:%S%.6f".to_string()
}

fn file_time_format_default() -> String {
    "%Y-%m-%d %H:%M:%S%.6f".to_string()
}

fn log_rotation_default() -> Rotation {
    Rotation::HOURLY
}