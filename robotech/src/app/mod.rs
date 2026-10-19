//! # 应用层（app）
//!
//! 提供应用级基础设施：
//! - 应用配置加载与热更新监听（`AppWatcher`）
//! - 应用层错误类型（`AppError`）
//! - ID 生成器的 worker node 分配与初始化（`setup_id_worker`）

mod app_error;
mod app_utils;
mod id_worker_utils;

// 重新导出结构体，简化外部引用
pub use app_error::*;
pub use app_utils::*;
pub use id_worker_utils::*;
