//! # 日志模块（log）
//!
//! 提供日志初始化与热更新能力：
//! - 日志配置（`LogConfig`）
//! - 日志模块错误类型（`LogError`）
//! - 日志系统初始化与配置热更新（`LogWatcher`）

mod log_config;
mod log_error;
mod log_utils;

// 重新导出结构体，简化外部引用
pub use log_config::*;
pub use log_error::*;
pub use log_utils::*;
