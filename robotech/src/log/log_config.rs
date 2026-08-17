use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing_appender::rolling::Rotation;
use wheel_rs::serde::rotation_serde;

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