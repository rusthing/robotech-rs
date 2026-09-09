use crate::cfg::CfgError;
use thiserror::Error;

/// # 自定义API客户端错误枚举
///
/// 该枚举定义了API客户端在执行HTTP请求过程中可能遇到的各种错误类型，
/// 涵盖配置、文件读取、网络请求、响应处理与数据解析等完整流程，
/// 并向调用方提供详细的错误信息。
///
/// ## 错误类型说明
/// - `Runtime`: 运行时错误（来自 anyhow）
/// - `Cfg`: 配置错误
/// - `ReadFile`: 文件读取失败，通常发生在加载配置文件或证书时
/// - `Request`: HTTP 请求发送失败，可能是网络连接问题或请求构建错误
/// - `Response`: 获取 HTTP 响应失败，通常是网络超时或连接中断
/// - `Jwt`: JWT 编码失败
/// - `NonSuccessStatus`: 响应状态码非 2xx
/// - `ParseJson`: JSON 格式响应解析失败
/// - `ParseBytes`: 字节流格式响应读取失败
/// - `SetApiClient` / `GetApiClient` / `NotInit`: API 客户端设置、获取或未初始化错误
#[derive(Error, Debug)]
pub enum ApiClientError {
    #[error("运行时错误: {0}")]
    Runtime(#[from] anyhow::Error),
    #[error("配置错误: {0}")]
    Cfg(#[from] CfgError),
    #[error("文件读取错误: {0}")]
    ReadFile(String, #[source] std::io::Error),
    #[error("请求失败:{0}")]
    Request(String, #[source] reqwest::Error),
    #[error("获取响应失败: {0}")]
    Response(String, #[source] reqwest::Error),
    #[error("JWT编码失败: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
    /// 响应状态非2xx
    ///
    /// 当服务器返回的状态码不在 2xx 范围内时触发此错误，
    /// 包括客户端错误（4xx）和服务端错误（5xx）。
    /// 此错误携带状态码和响应体信息，便于调试和处理。
    #[error("响应非2xx状态码: {0} -> {1}")]
    NonSuccessStatus(String, String),
    #[error("按Json格式解析响应失败: {0}")]
    ParseJson(String, #[source] serde_json::Error),
    #[error("按bytes格式解析响应失败: {0}")]
    ParseBytes(String, #[source] reqwest::Error),
    #[error("设置API客户端失败: {0}")]
    SetApiClient(String),
    #[error("获取API客户端失败: {0}")]
    GetApiClient(String),
    #[error("API客户端未初始化: {0}")]
    NotInit(String),
}
