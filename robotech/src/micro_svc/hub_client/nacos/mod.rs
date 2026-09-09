//! # Nacos 后端适配器
//!
//! 基于官方 `nacos-sdk` crate 实现配置中心（ConfigService）与注册中心
//! （NamingService）能力，详见 `nacos_client.rs` 顶部说明。

mod nacos_client;
mod nacos_config;

pub use nacos_client::*;
pub use nacos_config::*;
