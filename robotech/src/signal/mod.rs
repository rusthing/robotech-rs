//! # 信号模块（signal）
//!
//! 提供进程信号管理与 PID 文件控制：
//! - 信号管理器（`SignalManager`）
//! - 信号管理错误类型（`SignalManagerError`）

mod signal_manager;
mod signal_manager_error;

pub use signal_manager::*;
pub use signal_manager_error::*;
