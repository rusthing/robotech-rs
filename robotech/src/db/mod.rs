//! # 数据库模块
//!
//! 该模块负责数据库连接的配置、建立与管理，包括连接配置结构体（`DbConnConfig`）、
//! 数据库错误类型（`DbError`）以及全局数据库连接的管理工具（`db_utils`）。

mod db_conn_config;
mod db_error;
mod db_utils;

// 重新导出结构体，简化外部引用
pub use db_conn_config::DbConnConfig;
pub use db_error::*;
pub use db_utils::*;
