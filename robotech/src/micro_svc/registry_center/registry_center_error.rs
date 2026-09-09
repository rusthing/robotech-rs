use thiserror::Error;
use wheel_rs::ipnet_utils::IpnetError;

/// 注册中心的统一错误类型。
#[derive(Debug, Error)]
pub enum RegistryCenterError {
    /// 服务实例未找到
    #[error("服务实例未找到: {0}")]
    NotFound(String),

    /// 服务实例解析失败
    #[error("服务实例解析失败: {0}")]
    Parse(String),

    /// Web 服务器未启动（无法获取监听端口）
    #[error("Web服务器未启动")]
    WebServerNotRunning,

    /// 后端连接/请求失败
    #[error("后端连接/请求失败: {0}")]
    Connection(String),

    /// 后端未启用（对应 Cargo feature 未开启）
    #[error("不支持的后端: {0}（对应 Cargo feature 未启用，检查 Cargo.toml 里的 features）")]
    BackendNotEnabled(String),

    /// 获取本地 IP 失败
    #[error("获取本地IP失败: {0}")]
    IpnetError(#[from] IpnetError),

    /// 其它内部错误
    #[error("注册中心错误: {0}")]
    Internal(String),
}
