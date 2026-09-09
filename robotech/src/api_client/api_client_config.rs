//! # API配置模块
//!
//! 该模块定义了API相关的配置结构体

use serde::{Deserialize, Serialize};
use std::time::Duration;
use wheel_rs::serde::duration_serde;

/// # API 客户端配置键
///
/// 配置文件中的 `api` 段，用于读取 API 客户端配置。
pub const API_CLIENT_CONFIG_KEY: &str = "api";

/// # API 配置枚举
///
/// 通过 `type` 字段区分两种模式：
/// - `MicroSvc`: 服务发现模式，通过 svc_name 动态发现服务实例
/// - `Simple`: 静态直连模式，直接使用 base_url 连接
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ApiClientConfig {
    /// 微服务模式
    #[serde(rename_all = "kebab-case")]
    MicroSvc {
        /// 服务名，用于服务发现
        svc_name: String,
        /// 认证策略
        #[serde(default)]
        auth: Option<ApiAuthStrategy>,
        /// 最大失败次数，超过后进入冷却期，默认 3
        #[serde(default = "default_max_failures")]
        max_failures: usize,
        /// 失败冷却时间，默认 30s
        #[serde(default = "default_cooldown_duration", with = "duration_serde")]
        cooldown_duration: Duration,
        /// 服务发现刷新间隔，默认 30s
        #[serde(default = "default_refresh_interval", with = "duration_serde")]
        refresh_interval: Duration,
    },
    /// 简单直连模式
    #[serde(rename_all = "kebab-case")]
    Simple {
        /// API请求的基础URL，例如: http://127.0.0.1:8080
        base_url: String,
        /// 认证策略
        #[serde(default)]
        auth: Option<ApiAuthStrategy>,
    },
}

fn default_max_failures() -> usize {
    3
}

fn default_cooldown_duration() -> Duration {
    Duration::from_secs(30)
}

fn default_refresh_interval() -> Duration {
    Duration::from_secs(30)
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
