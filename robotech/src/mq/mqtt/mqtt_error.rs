use idworker::IdWorkerError;
use rumqttc::ClientError;
use thiserror::Error;

/// MQTT 订阅过程中的错误类型。
#[derive(Error, Debug)]
pub enum MqttError {
    /// MQTT 请求失败（底层客户端错误）
    #[error("MQTT请求失败: {0}")]
    Request(#[from] ClientError),
    /// 消息处理失败
    #[error("MQTT消息处理失败: {0}")]
    Handle(String),
    /// ID 生成器错误
    #[error("ID工作者错误: {0}")]
    IdWorker(#[from] IdWorkerError),
}
