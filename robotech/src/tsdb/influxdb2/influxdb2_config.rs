use crate::mq::mqtt::qos_serde;
use rumqttc::QoS;
use serde::Deserialize;
use std::time::Duration;
use wheel_rs::serde::{duration_serde, vec_serde};

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Influxdb2Config {
    /// 数据库URL
    #[serde()]
    pub url: String,
    /// 组织名称
    #[serde()]
    pub org: String,
    /// 桶
    #[serde()]
    pub bucket: String,
    /// 数据库token
    #[serde()]
    pub token: String,
}
