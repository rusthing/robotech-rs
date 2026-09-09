//! # 控制器模块
//!
//! 提供控制器层的错误类型（`CtrlError`）与从请求头解析用户 ID 等工具函数。

mod ctrl_error;
/// 控制器工具函数模块
pub mod ctrl_utils;

pub use ctrl_error::*;
