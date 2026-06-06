use crate::mq::mqtt::qos_serde;
use rumqttc::QoS;
use serde::Deserialize;
use std::time::Duration;
use wheel_rs::serde::{duration_serde, vec_serde};

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct MqttConfig {
    /// 客户端ID(唯一，接收离线消息的条件之一)
    #[serde()]
    pub client_id: String,
    /// mqtt服务器地址
    #[serde()]
    pub host: String,
    /// mqtt服务器端口
    #[serde()]
    pub port: u16,
    /// mqtt服务器保持连接时间间隔
    #[serde(with = "duration_serde", default = "default_keep_alive")]
    pub keep_alive: Duration,
    /// 是否清理会话(默认不清理，是接收离线消息的条件之一)
    #[serde(default = "default_clean_session")]
    pub clean_session: bool,
    /// mqtt服务器用户名
    #[serde()]
    pub username: String,
    /// mqtt服务器密码
    #[serde()]
    pub password: String,
    /// mqtt服务器消息缓存容量
    #[serde(default = "default_cap")]
    pub cap: usize,
    /// mqtt服务器消息主题
    #[serde(with = "vec_serde")]
    pub topic: Vec<String>,
    /// mqtt服务器消息QoS等级(默认1，大于1是接收离线消息的条件之一)
    #[serde(with = "qos_serde", default = "default_qos")]
    pub qos: QoS,
    /// mqtt服务器断开重连时间间隔
    #[serde(with = "duration_serde", default = "default_reconnect_interval")]
    pub reconnect_interval: Duration,
}

fn default_keep_alive() -> Duration {
    Duration::from_secs(60)
}
fn default_clean_session() -> bool {
    false
}
fn default_cap() -> usize {
    1024
}
fn default_qos() -> QoS {
    QoS::AtLeastOnce
}
fn default_reconnect_interval() -> Duration {
    Duration::from_secs(30)
}
