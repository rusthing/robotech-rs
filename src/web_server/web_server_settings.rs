use serde::{Deserialize, Serialize};
use wheel_rs::serde::vec_option_serde;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct WebServerSettings {
    /// 绑定的IP地址
    #[serde(with = "vec_option_serde", default = "bind_default")]
    pub bind: Option<Vec<String>>,
    /// Web服务器的端口号
    #[serde(default = "port_default")]
    pub port: Option<u16>,

    /// 监听地址(ip+':'+port，例如127.0.0.1:80或\[::\]:80)
    #[serde(with = "vec_option_serde", default = "listen_default")]
    pub listen: Option<Vec<String>>,

    /// 是否支持健康检查
    #[serde(default = "support_health_check_default")]
    pub support_health_check: bool,
}

impl Default for WebServerSettings {
    fn default() -> Self {
        WebServerSettings {
            bind: bind_default(),
            port: port_default(),
            listen: listen_default(),
            support_health_check: support_health_check_default(),
        }
    }
}

fn bind_default() -> Option<Vec<String>> {
    None
}
fn port_default() -> Option<u16> {
    Some(0)
}

fn listen_default() -> Option<Vec<String>> {
    None
}
fn support_health_check_default() -> bool {
    true
}
