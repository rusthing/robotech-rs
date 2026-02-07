use thiserror::Error;

/// # 自定义API客户端错误枚举
///
/// 该枚举定义了API客户端在执行HTTP请求过程中可能遇到的各种错误类型。
/// 这些错误涵盖了从文件读取、网络请求、响应处理到数据解析等完整流程中
/// 可能出现的异常情况，并提供详细的错误信息反馈给调用方。
///
/// ## 错误类型说明
/// - `FileError`: 文件读取操作失败，通常发生在加载配置文件或证书时
/// - `RequestError`: HTTP请求发送失败，可能是网络连接问题或请求构建错误
/// - `ResponseError`: 获取HTTP响应失败，通常是网络超时或连接中断
/// - `ResponseStatusError`: HTTP响应状态码表示错误，如4xx客户端错误或5xx服务器错误
/// - `JsonParseError`: JSON格式响应解析失败
/// - `BytesParseError`: 字节流格式响应解析失败
#[derive(Error, Debug)]
pub enum ApiClientError {
    #[error("文件读取错误: {0}")]
    ReadFile(String, #[source] std::io::Error),
    #[error("请求失败:{0}")]
    Request(String, #[source] reqwest::Error),
    #[error("获取响应失败: {0}")]
    Response(String, #[source] reqwest::Error),
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
}
