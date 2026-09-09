//! # Web 模块
//!
//! 提供 HTTP Web 服务相关的配置与运行能力，包括 CORS、控制器错误、
//! 健康检查、HTTPS、访问控制中间件，以及 Web 服务器的配置与启停。

mod cors;
mod ctrl;
mod health_check;
mod https;
/// 中间件模块（IP 拦截、禁止访问 URN、仅本地访问限制等）
pub mod middleware;
mod server;

// 重新导出结构体，简化外部引用
pub(crate) use cors::*;
pub use ctrl::*;
pub(crate) use health_check::*;
pub(crate) use https::*;
pub use server::*;
