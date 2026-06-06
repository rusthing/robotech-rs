use rumqttc::QoS;
use serde::{Deserialize, Deserializer};

/// Custom deserializer for rumqttc::QoS
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
