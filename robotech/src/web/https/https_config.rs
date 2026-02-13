use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct HttpsConfig {
    /// 是否启用(默认: false)
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    /// 证书文件路径
    #[serde()]
    pub cert_path: Option<String>,
    /// 密钥文件路径
    #[serde()]
    pub key_path: Option<String>,
    /// 是否重定向HTTP请求到HTTPS(默认: false)
    #[serde(default = "redirect_http_to_https_default")]
    pub redirect_http_to_https: bool,
    /// 重定向HTTP请求到HTTPS的端口
    #[serde()]
    pub redirect_port: Option<u16>,
}

impl Default for HttpsConfig {
    fn default() -> Self {
        HttpsConfig {
            enabled: enabled_default(),
            cert_path: None,
            key_path: None,
            redirect_http_to_https: redirect_http_to_https_default(),
            redirect_port: None,
        }
    }
}
fn enabled_default() -> bool {
    true
}

fn redirect_http_to_https_default() -> bool {
    false
}
