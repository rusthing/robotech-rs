//! # CORS 模块
//!
//! 提供跨域资源共享（CORS）的配置结构体与中间件构建函数。

mod cors_config;
mod cors_utils;

pub use cors_config::*;
pub use cors_utils::*;
