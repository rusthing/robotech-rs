use crate::log::LogConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wheel_rs::serde::duration_serde;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BaseConfig {
    /// 环境(dev/test/prod)
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub log: Option<LogConfig>,
    /// 监听防抖延迟时间
    #[serde(with = "duration_serde", default = "watch_debounce_delay_default")]
    pub watch_debounce_delay: Duration,
}

fn watch_debounce_delay_default() -> Duration {
    Duration::from_secs(3)
}
