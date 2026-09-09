//! # 应用层（app）
//!
//! 提供应用级基础设施：
//! - 应用配置加载与热更新监听（`AppWatcher`）
//! - 应用层错误类型（`AppError`）

mod app_error;
mod app_utils;

// 重新导出结构体，简化外部引用
pub use app_error::*;
pub use app_utils::*;
