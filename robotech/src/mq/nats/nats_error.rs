use thiserror::Error;

/// # NATS 操作错误类型
///
/// 定义 NATS 消息操作过程中可能出现的各种错误类型，用于统一处理
/// 连接、发布、订阅、消息处理及序列化等场景的异常情况。
#[derive(Error, Debug)]
pub enum NatsError {
    /// NATS 连接失败
    ///
    /// 当 NATS 客户端初始连接或重连失败时触发此错误。
    ///
    /// ## 参数
    /// - `0`: 连接失败的具体原因描述。
    #[error("NATS 连接错误: {0}")]
    Connect(String),

    /// NATS 发布消息失败
    ///
    /// 当向指定 Subject 发布消息失败时触发此错误，
    /// 包括 Core NATS 发布和 JetStream 发布的异常。
    ///
    /// ## 参数
    /// - `0`: 发布失败的具体原因描述。
    #[error("NATS 发布错误: {0}")]
    Publish(String),

    /// NATS 订阅失败
    ///
    /// 当订阅指定 Subject 或创建 Consumer 失败时触发此错误。
    ///
    /// ## 参数
    /// - `0`: 订阅失败的具体原因描述。
    #[error("NATS 订阅错误: {0}")]
    Subscribe(String),

    /// 消息处理失败
    ///
    /// 当用户提供的消息处理函数返回错误时触发此错误。
    ///
    /// ## 参数
    /// - `0`: 处理失败的具体原因描述。
    #[error("NATS 消息处理失败: {0}")]
    Handle(String),

    /// 序列化/反序列化失败
    ///
    /// 当消息负载的 JSON 序列化或反序列化失败时触发此错误，
    /// 由 [`serde_json::Error`] 通过 `From` 自动转换而来。
    #[error("NATS 序列化错误: {0}")]
    Serde(#[from] serde_json::Error),
}