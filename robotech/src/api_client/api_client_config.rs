//! # API配置模块
//!
//! 该模块定义了API相关的配置结构体

use serde::{Deserialize, Serialize};
use std::time::Duration;
use wheel_rs::serde::duration_serde;

pub const API_CLIENT_CONFIG_KEY: &str = "api";

/// # API配置结构体
///
/// 用于存储API所需的各种配置参数
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ApiClientConfig {
    /// API请求的基础URL
    ///
    /// 例如: http://127.0.0.1:8080
    #[serde(default)]
    pub base_url: Option<String>,
    /// 服务名，用于 feign 模式（服务发现）
    ///
    /// 设置后优先使用服务发现模式，base_url 作为 fallback
    #[serde(default)]
    pub svc_name: Option<String>,
    /// 认证策略
    ///
    /// 用于定义API请求的认证策略，包括 Token、Basic、Bearer 等
    #[serde(default)]
    pub auth: Option<ApiAuthStrategy>,
}

/// # API认证策略枚举
///
/// 用于定义API请求的认证策略
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ApiAuthStrategy {
    /// Token 认证策略
    ///
    /// 用于在请求头中包含认证令牌，通常用于 API 密钥或 JWT 认证
    #[serde(rename_all = "kebab-case")]
    Token {
        /// 认证头名称
        header: String,
        /// 认证令牌
        token: String,
    },
    /// Basic 认证策略
    ///
    /// 用于在请求头中包含用户名和密码，通常用于 HTTP 基本认证
    #[serde(rename_all = "kebab-case")]
    Basic {
        /// 用户名
        username: String,
        /// 密码
        password: Option<String>,
    },
    /// Bearer 认证策略
    ///
    /// 用于在请求头中包含 Bearer 令牌，通常用于 JWT 认证
    #[serde(rename_all = "kebab-case")]
    Bearer {
        /// 算法
        algorithm: String,
        /// 私钥(用于生成JWT)
        private_key: String,
        /// 主题(通常是用户ID)
        sub: String,
        /// 发布者(通常是API服务端)
        iss: String,
        /// 过期时间（Duration）
        #[serde(with = "duration_serde")]
        expires_in: Duration,
    },
}

/// # JWT声明结构体
///
/// 用于存储JWT的声明信息，包括主题、发布者、签发时间、过期时间等
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Claim {
    /// 主题(通常是用户ID)
    pub sub: String,
    /// 发布者(通常是API服务端)
    pub iss: String,
    /// 签发时间（Unix时间戳）
    pub iat: i64,
    /// 过期时间（Unix时间戳）
    pub exp: i64,
}
