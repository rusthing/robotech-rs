use thiserror::Error;

/// Hub 客户端（后端连接层）的统一错误类型。
#[derive(Debug, Error)]
pub enum HubClientError {
    /// 后端连接/请求失败
    #[error("后端连接/请求失败: {0}")]
    Connection(String),
    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),
    /// 解析错误
    #[error("解析错误: {0}")]
    Parse(String),
}
