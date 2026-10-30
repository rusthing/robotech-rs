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

    /// 永久性处理失败：重投多少次结果都一样，必须直接 ACK 丢弃
    ///
    /// 与 [`RabbitMqError::Handle`] 的唯一区别是**重投有没有价值**：
    ///
    /// - `Handle`：临时故障（数据库抖一下、下游重启、超时）。消费侧会退避后
    ///   重新入队，重投几次就过去了；
    /// - `Discard`：这条消息本身报不上去（事件策略被停用、报文参数不合法、
    ///   结构对不上）。再投一万次也是同一个结果，重投只会让它占着队头空转，
    ///   **把后面所有正常消息都堵在后面**——这是单消费者队列最常见的「整体
    ///   失效」成因，比慢严重得多。
    ///
    /// ## 参数
    /// - `0`: 丢弃的具体原因描述（会原样进 error 日志，要写清楚是什么消息）。
    #[error("RabbitMQ 消息永久失败（已确认丢弃）: {0}")]
    Discard(String),

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

impl RabbitMqError {
    /// # 这条错误还值不值得重投
    ///
    /// 只有 [`RabbitMqError::Discard`] 是「投了也白投」；其余都是临时故障
    /// （连接抖动、序列化偶发、下游不可用），重投有恢复的可能。
    ///
    /// 默认把不认识的情况一律当**可重投**：误判成永久会丢掉一条真实消息，
    /// 误判成临时只是多转几圈——两害相权，宁可多重投。
    pub fn is_permanent(&self) -> bool {
        matches!(self, RabbitMqError::Discard(_))
    }
}