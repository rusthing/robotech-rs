use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct HealthCheckConfig {
    /// 是否暴露健康检查(默认暴露，允许Consul进行健康检查)
    #[serde(default = "exposed_default")]
    pub exposed: bool,
    /// 健康检查的uri(默认/actuator/health，兼容SpringCloud)
    #[serde(default = "uri_default")]
    pub uri: String,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            exposed: exposed_default(),
            uri: uri_default(),
        }
    }
}

fn exposed_default() -> bool {
    true
}

fn uri_default() -> String {
    "/actuator/health".to_string()
}