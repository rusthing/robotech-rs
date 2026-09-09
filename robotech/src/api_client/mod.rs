//! # API 客户端模块
//!
//! 提供 API 客户端（feign/直连）的配置、错误类型、请求工具与 webhook 配置。

mod api_client_config;
mod api_client_error;
mod api_client_utils;
mod webhook_config;

// 重新导出结构体，简化外部引用
pub use api_client_config::*;
pub use api_client_error::*;
pub use api_client_utils::*;
pub use webhook_config::*;
