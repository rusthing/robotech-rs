use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct HttpsConfig {
    /// 是否启用(默认: false)
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    /// 证书文件路径
    #[serde()]
    pub cert: Option<String>,
    /// 密钥文件路径
    #[serde()]
    pub key: Option<String>,
}

impl Default for HttpsConfig {
    fn default() -> Self {
        HttpsConfig {
            enabled: enabled_default(),
            cert: None,
            key: None,
        }
    }
}
fn enabled_default() -> bool {
    true
}
