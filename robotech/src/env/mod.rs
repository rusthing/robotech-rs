//! # 环境（ENV）模块
//!
//! 该模块负责应用运行环境的初始化，包括可执行文件路径信息的获取
//! （`env_utils`）与环境错误类型（`env_error`）。

mod env_error;
mod env_utils;

// 重新导出结构体，简化外部引用
pub use env_error::*;
pub use env_utils::*;
