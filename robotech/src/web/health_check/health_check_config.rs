use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct HealthCheckConfig {
    /// 是否暴露健康检查(默认不暴露，只能本地访问)
    #[serde(default)]
    pub exposed: bool,
    /// 健康检查的uri(默认/health)
    #[serde(default = "uri_default")]
    pub uri: String,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            exposed: false,
            uri: uri_default(),
        }
    }
}

fn uri_default() -> String {
    "/health".to_string()
}
