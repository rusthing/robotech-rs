//! # 注册中心模块
//!
//! 定义注册中心的统一契约（`RegistryCenterClient`）、服务实例结构（`ServiceInstance`）、
//! 定位信息（`RegistryKey`）与错误类型（`RegistryCenterError`）。
//! 具体后端（Consul / Etcd / Nacos）在 `hub_client` 各子模块中实现该契约。

mod registry_center_client;
mod registry_center_config;
mod registry_center_eo;
mod registry_center_error;

pub use registry_center_client::*;
pub use registry_center_config::*;
pub use registry_center_eo::*;
pub use registry_center_error::*;
