mod config_utils;
mod config_error;

// 重新导出结构体，简化外部引用
pub use config_utils::parse_config;
pub use config_error::ConfigError;
