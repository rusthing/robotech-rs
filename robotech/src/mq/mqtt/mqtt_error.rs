use rumqttc::ClientError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MqttError {
    #[error("MQTT请求失败: {0}")]
    Request(#[from] ClientError),
}
