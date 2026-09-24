use thiserror::Error;

/// RabbitMQ 操作错误类型
///
/// 定义 RabbitMQ 消息操作过程中可能出现的各种错误类型，用于统一处理
/// 连接、发布、消费、消息处理及序列化等场景的异常情况。
#[derive(Error, Debug)]
pub enum RabbitMqError {
    /// RabbitMQ 连接失败
    ///
    /// 当 RabbitMQ 客户端初始连接或重连失败时触发此错误。
    ///
    /// ## 参数
    /// - `0`: 连接失败的具体原因描述。
    #[error("RabbitMQ 连接错误: {0}")]
    Connect(String),

    /// RabbitMQ 发布消息失败
    ///
    /// 当向指定 Exchange 发布消息失败时触发此错误。
    ///
    /// ## 参数
    /// - `0`: 发布失败的具体原因描述。
    #[error("RabbitMQ 发布错误: {0}")]
    Publish(String),

    /// RabbitMQ 消费失败
    ///
    /// 当消费指定队列消息失败时触发此错误。
    ///
    /// ## 参数
    /// - `0`: 消费失败的具体原因描述。
    #[error("RabbitMQ 消费错误: {0}")]
    Consume(String),

    /// 消息处理失败
    ///
    /// 当用户提供的消息处理函数返回错误时触发此错误。
    ///
    /// ## 参数
    /// - `0`: 处理失败的具体原因描述。
    #[error("RabbitMQ 消息处理失败: {0}")]
    Handle(String),

    /// 序列化/反序列化失败
    ///
    /// 当消息负载的 JSON 序列化或反序列化失败时触发此错误，
    /// 由 [`serde_json::Error`] 通过 `From` 自动转换而来。
    #[error("RabbitMQ 序列化错误: {0}")]
    Serde(#[from] serde_json::Error),

    /// Channel 操作失败
    ///
    /// 当 Channel 创建、Exchange/Queue 声明等操作失败时触发此错误。
    ///
    /// ## 参数
    /// - `0`: 操作失败的具体原因描述。
    #[error("RabbitMQ Channel 错误: {0}")]
    Channel(String),
}