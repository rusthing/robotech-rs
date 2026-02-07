#[cfg(feature = "api-client")]
use crate::api_client::ApiClientError;
use crate::dao::DaoError;
use std::time::SystemTimeError;

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
pub enum SvcError {
    #[error("{0}")]
    Runtime(#[from] anyhow::Error),
    #[error("系统时间错误: {0}")]
    SystemTime(#[from] SystemTimeError),
    #[error("参数校验错误: {0}")]
    Validation(#[from] validator::ValidationError),
    #[error("参数校验错误: {0}")]
    Validations(#[from] validator::ValidationErrors),
    #[error("找不到数据: {0}")]
    NotFound(String),
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    #[cfg(feature = "db")]
    #[error("数据库错误: {0}")]
    Database(#[from] DaoError),
    #[cfg(feature = "api-client")]
    #[error("API客户端错误, {0}")]
    ApiClient(#[from] ApiClientError),
}
