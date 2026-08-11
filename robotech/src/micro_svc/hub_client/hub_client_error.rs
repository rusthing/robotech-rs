use thiserror::Error;

#[derive(Debug, Error)]
pub enum HubClientError {
    #[error("后端连接/请求失败: {0}")]
    Connection(String),
    #[error("配置错误: {0}")]
    Config(String),
    #[error("解析错误: {0}")]
    Parse(String),
}
