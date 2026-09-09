//! # 配置（CFG）模块
//!
//! 该模块负责应用配置的加载与反序列化，包括基础配置结构体（`BaseConfig`）、
//! 配置错误类型（`CfgError`）以及配置构建工具（`cfg_utils`）。

mod base_config;
mod cfg_error;
mod cfg_utils;

pub use base_config::*;
pub use cfg_error::*;
pub use cfg_utils::*;
