use crate::mq::mqtt::qos_serde;
use rumqttc::QoS;
use serde::Deserialize;
use std::time::Duration;
use wheel_rs::serde::{duration_serde, vec_serde};

/// MQTT 连接与订阅配置。
///
/// 从配置文件中反序列化得到；`client_id`、`clean_session`、`qos` 共同决定
/// 是否能在断线重连后收到离线消息。
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct MqttConfig {
    /// 客户端ID(唯一，接收离线消息的条件之一)
    pub client_id: String,
    /// mqtt服务器地址
    pub host: String,
    /// mqtt服务器端口
    pub port: u16,
    /// mqtt服务器保持连接时间间隔
    #[serde(with = "duration_serde", default = "default_keep_alive")]
    pub keep_alive: Duration,
    /// 是否清理会话(默认不清理，是接收离线消息的条件之一)
    #[serde(default = "default_clean_session")]
    pub clean_session: bool,
    /// mqtt服务器用户名
    pub username: Option<String>,
    /// mqtt服务器密码
    pub password: Option<String>,
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
    /// 处理消息错误睡眠时间
    #[serde(with = "duration_serde", default = "default_handle_error_sleep")]
    pub handle_error_sleep: Duration,
    /// 应答消息错误睡眠时间
    #[serde(with = "duration_serde", default = "default_ack_error_sleep")]
    pub ack_error_sleep: Duration,
    /// 是否抛弃处理错误的消息
    #[serde(default = "default_discard_error")]
    pub discard_error: bool,
    /// 最大入站报文大小(默认 10KB，超过会导致连接中断)
    #[serde(default = "default_max_incoming_packet_size")]
    pub max_incoming_packet_size: usize,
    /// 最大出站报文大小(默认 10KB)
    #[serde(default = "default_max_outgoing_packet_size")]
    pub max_outgoing_packet_size: usize,
    /// 是否手动应答(默认手动应答)
    #[serde(default = "default_manual_acks")]
    pub manual_acks: bool,
    /// 最大在途消息数量(默认 100)
    #[serde(default = "default_inflight")]
    pub inflight: u16,
    /// 请求(publish/subscribe)通道容量(默认 10)
    #[serde(default = "default_request_channel_capacity")]
    pub request_channel_capacity: usize,
    /// 待发送消息节流间隔(默认 0，不节流)
    #[serde(with = "duration_serde", default = "default_pending_throttle")]
    pub pending_throttle: Duration,
    /// 遗嘱消息配置(默认无遗嘱消息)
    #[serde(default)]
    pub last_will: Option<LastWillConfig>,
}

/// MQTT 遗嘱消息配置。
///
/// 客户端异常断开时由 broker 代为发布，用于通知其它订阅者该客户端已离线。
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct LastWillConfig {
    /// 遗嘱消息主题
    pub topic: String,
    /// 遗嘱消息内容
    pub message: String,
    /// 遗嘱消息QoS等级
    #[serde(with = "qos_serde", default = "default_qos")]
    pub qos: QoS,
    /// 遗嘱消息是否保留
    #[serde(default)]
    pub retain: bool,
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
    QoS::AtMostOnce
}
fn default_reconnect_interval() -> Duration {
    Duration::from_secs(30)
}
fn default_handle_error_sleep() -> Duration {
    Duration::from_secs(1)
}
fn default_ack_error_sleep() -> Duration {
    Duration::from_secs(1)
}
fn default_discard_error() -> bool {
    false
}
fn default_max_incoming_packet_size() -> usize {
    10 * 1024
}
fn default_max_outgoing_packet_size() -> usize {
    10 * 1024
}
fn default_manual_acks() -> bool {
    true
}
fn default_inflight() -> u16 {
    100
}
fn default_request_channel_capacity() -> usize {
    10
}
fn default_pending_throttle() -> Duration {
    Duration::from_millis(0)
}
