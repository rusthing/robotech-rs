use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct WebServerSettings {
    /// 绑定的IP地址
    #[serde(default = "bind_default")]
    pub bind: Vec<String>,
    /// Web服务器的端口号
    #[serde(default = "port_default")]
    pub port: Option<u16>,
}

impl Default for WebServerSettings {
    fn default() -> Self {
        WebServerSettings {
            bind: bind_default(),
            port: port_default(),
        }
    }
}

fn bind_default() -> Vec<String> {
    vec![String::from("0.0.0.0")]
}

fn port_default() -> Option<u16> {
    Some(0)
}
