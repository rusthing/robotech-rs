use log::error;

/// # 自定义服务层的错误枚举
///
/// 该枚举定义了服务层可能遇到的各种错误类型，包括数据未找到、重复键约束违反、
/// IO错误和数据库错误。这些错误类型用于在服务层统一处理各种异常情况，
/// 并提供清晰的错误信息反馈给调用方。
///
/// ## 错误类型说明
/// - `NotFound`: 表示请求的数据未找到，通常用于查询操作
/// - `DuplicateKey`: 表示违反了唯一性约束，如重复的用户名或邮箱
/// - `IoError`: 表示输入输出相关的错误，如文件读写失败
/// - `DatabaseError`: 表示底层数据库操作发生的错误
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}文件读取错误")]
    FileError(String, #[source] std::io::Error),
    #[error("{0}: 请求失败")]
    RequestError(String, #[source] reqwest::Error),
    #[error("{0}: 获取响应失败")]
    ResponseError(String, #[source] reqwest::Error),
    #[error("{0}: 响应状态错误->{1}")]
    ResponseStatusError(String, String),
    #[error("{0}: 按Json格式解析响应失败")]
    JsonParseError(String, #[source] serde_json::Error),
    #[error("{0}: 按bytes格式解析响应失败")]
    BytesParseError(String, #[source] reqwest::Error),
}
