pub mod db_config;
pub mod db_utils;

// 重新导出结构体，简化外部引用
pub use db_config::DbConfig;
pub use db_utils::{init_db, DB_CONN};
