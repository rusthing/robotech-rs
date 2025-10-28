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

    /// 监听地址(ip+':'+port，例如127.0.0.1:80或\[::\]:80)
    #[serde(default = "listen_default")]
    pub listen: Vec<String>,
}

impl Default for WebServerSettings {
    fn default() -> Self {
        WebServerSettings {
            bind: bind_default(),
            port: port_default(),
            listen: listen_default(),
        }
    }
}

fn bind_default() -> Vec<String> {
    vec![String::from("::")]
}
fn port_default() -> Option<u16> {
    Some(0)
}

fn listen_default() -> Vec<String> {
    vec![]
}
