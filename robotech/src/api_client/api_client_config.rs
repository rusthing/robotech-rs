//! # API配置模块
//!
//! 该模块定义了API相关的配置结构体
use serde::{Deserialize, Serialize};

/// # API配置结构体
///
/// 用于存储API所需的各种配置参数
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ApiClientConfig {
    /// API请求的基础URL
    ///
    /// 例如: http://127.0.0.1:8080
    #[serde()]
    pub base_url: String,
}
