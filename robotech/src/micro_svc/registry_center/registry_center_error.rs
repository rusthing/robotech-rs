use thiserror::Error;
use wheel_rs::ipnet_utils::IpnetError;

#[derive(Debug, Error)]
pub enum RegistryCenterError {
    #[error("服务实例未找到: {0}")]
    NotFound(String),

    #[error("服务实例解析失败: {0}")]
    Parse(String),

    #[error("Web服务器未启动")]
    WebServerNotRunning,

    #[error("后端连接/请求失败: {0}")]
    Connection(String),

    #[error("不支持的后端: {0}（对应 Cargo feature 未启用，检查 Cargo.toml 里的 features）")]
    BackendNotEnabled(String),

    #[error("获取本地IP失败: {0}")]
    IpnetError(#[from] IpnetError),

    #[error("注册中心错误: {0}")]
    Internal(String),
}
