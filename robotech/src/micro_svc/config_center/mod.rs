//! # 配置中心模块
//!
//! 定义配置中心的统一契约（`ConfigCenterClient`）、配置定位结构（`ConfigKey`）、
//! 配置内容（`ConfigItem`）与错误类型（`ConfigCenterError`）。
//! 具体后端（Consul / Etcd / Nacos）在 `hub_client` 各子模块中实现该契约。

mod config_center_client;
mod config_center_config;
mod config_center_eo;
mod config_center_error;

pub use config_center_client::*;
pub use config_center_config::*;
pub use config_center_eo::*;
pub use config_center_error::*;
