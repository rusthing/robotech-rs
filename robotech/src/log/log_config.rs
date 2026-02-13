use serde::{Deserialize, Serialize};
use tracing_appender::rolling::Rotation;
use wheel_rs::serde::rotation_serde;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct LogConfig {
    #[serde(default = "level_default")]
    pub level: String,
    #[serde(default = "console_time_format_default")]
    pub console_time_format: String,
    #[serde(default = "file_time_format_default")]
    pub file_time_format: String,
    #[serde(with = "rotation_serde", default = "log_rotation_default")]
    pub rotation: Rotation,
    #[serde(default = "spans_config_default")]
    pub show_spans: bool,
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

fn spans_config_default() -> bool {
    true
}
