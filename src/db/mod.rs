pub mod db_settings;
pub mod db_utils;

// 重新导出结构体，简化外部引用
pub use db_settings::DbSettings;
pub use db_utils::{init_db, DB_CONN};
