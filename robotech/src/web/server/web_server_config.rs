use crate::web::cors::CorsConfig;
use crate::web::https::HttpsConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wheel_rs::serde::duration_serde;
use wheel_rs::serde::vec_serde;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct WebServerConfig {
    /// 绑定的IP地址
    #[serde(with = "vec_serde", default = "bind_default")]
    pub bind: Vec<String>,
    /// Web服务器的端口号(默认0)
    #[serde(default = "port_default")]
    pub port: Option<u16>,

    /// 监听地址列表(监听地址格式: ip+':'+port，例如127.0.0.1:80或\[::\]:80)
    #[serde(with = "vec_serde", default = "listen_default")]
    pub listen: Vec<String>,

    /// 是否启用端口复用(默认关闭)
    ///
    /// * 启用端口复用是为了实现无缝重启服务器，发指令重启服务器时，会在新的服务器启动完成后，才会关闭旧的服务器，达到无缝重启服务器的效果
    /// * 如果绑定监听的是随机端口，会自动禁用，因为随机端口新旧服务器的端口就不会冲突
    /// * 如果启用，同时也会开启支持健康检查，因为重启时需要健康检查来判断新的服务器是否启动成功，才停止旧的服务器
    #[serde(default = "reuse_port_default")]
    pub reuse_port: bool,

    /// 是否启用Https(默认关闭)
    #[serde(default)]
    pub https: Option<HttpsConfig>,

    /// CORS配置(不设置默认不开启)
    #[serde(default)]
    pub cors: Option<CorsConfig>,

    /// 是否支持健康检查
    /// 如果绑定或监听随机端口，或是启用端口复用，都会自动启用支持健康检查，因为重启时需要健康检查来判断新的服务器是否启动成功，才停止旧的服务器
    #[serde(default = "support_health_check_default")]
    pub support_health_check: bool,

    #[serde(with = "duration_serde", default = "start_wait_timeout_default")]
    pub start_wait_timeout: Duration,

    #[serde(with = "duration_serde", default = "start_retry_interval_default")]
    pub start_retry_interval: Duration,

    #[serde(
        with = "duration_serde",
        default = "terminate_old_wait_timeout_default"
    )]
    pub terminate_old_wait_timeout: Duration,

    #[serde(
        with = "duration_serde",
        default = "terminate_old_retry_interval_default"
    )]
    pub terminate_old_retry_interval: Duration,
}

impl Default for WebServerConfig {
    fn default() -> Self {
        WebServerConfig {
            bind: bind_default(),
            port: port_default(),
            listen: listen_default(),
            reuse_port: reuse_port_default(),
            https: None,
            cors: None,
            support_health_check: support_health_check_default(),
            start_wait_timeout: start_wait_timeout_default(),
            start_retry_interval: start_retry_interval_default(),
            terminate_old_wait_timeout: terminate_old_wait_timeout_default(),
            terminate_old_retry_interval: terminate_old_retry_interval_default(),
        }
    }
}

fn bind_default() -> Vec<String> {
    vec![]
}
fn port_default() -> Option<u16> {
    None
}

fn listen_default() -> Vec<String> {
    vec![]
}

fn reuse_port_default() -> bool {
    false
}

fn support_health_check_default() -> bool {
    true
}

fn start_wait_timeout_default() -> Duration {
    Duration::from_secs(10)
}
fn start_retry_interval_default() -> Duration {
    Duration::from_millis(500)
}
fn terminate_old_wait_timeout_default() -> Duration {
    Duration::from_secs(15)
}
fn terminate_old_retry_interval_default() -> Duration {
    Duration::from_millis(500)
}
