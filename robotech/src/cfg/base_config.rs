use crate::log::LogConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wheel_rs::serde::duration_serde;

/// # 基础配置结构体
///
/// 用于存储应用的基础配置项，包括应用名称、运行环境、日志配置
/// 以及配置文件监听防抖延迟时间。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BaseConfig {
    /// 应用名称
    #[serde(default)]
    pub app_name: Option<String>,
    /// 环境(dev/test/prod)
    #[serde(default)]
    pub profile: Option<String>,
    /// 日志配置
    #[serde(default)]
    pub log: Option<LogConfig>,
    /// 监听防抖延迟时间
    #[serde(with = "duration_serde", default = "watch_debounce_delay_default")]
    pub watch_debounce_delay: Duration,
}

fn watch_debounce_delay_default() -> Duration {
    Duration::from_secs(3)
}
