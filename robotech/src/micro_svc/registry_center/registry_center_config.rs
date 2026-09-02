use serde::{Deserialize, Serialize};
use std::time::Duration;
use wheel_rs::serde::duration_serde;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RegistryCenterConfig {
    #[serde(with = "duration_serde", default = "retry_interval_default")]
    pub retry_interval: Duration,
    #[serde(with = "duration_serde", default = "refresh_interval_default")]
    pub refresh_interval: Duration,
}

impl Default for RegistryCenterConfig {
    fn default() -> Self {
        Self {
            retry_interval: retry_interval_default(),
            refresh_interval: refresh_interval_default(),
        }
    }
}

fn retry_interval_default() -> Duration {
    Duration::from_secs(3)
}
fn refresh_interval_default() -> Duration {
    Duration::from_secs(30)
}
