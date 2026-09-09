use rumqttc::QoS;
use serde::{Deserialize, Deserializer};

/// `rumqttc::QoS` 的自定义反序列化器：将整数 0/1/2 映射为对应的 QoS 等级。
pub fn deserialize<'de, D>(deserializer: D) -> Result<QoS, D::Error>
where
    D: Deserializer<'de>,
{
    let qos_value: u8 = Deserialize::deserialize(deserializer)?;
    match qos_value {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => Err(serde::de::Error::custom(format!(
            "Invalid QoS value: {}. Must be 0, 1, or 2.",
            qos_value
        ))),
    }
}
