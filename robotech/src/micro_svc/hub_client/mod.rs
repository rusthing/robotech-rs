//! # Hub 客户端模块
//!
//! `HubClient` 是配置中心与注册中心的统一门面：聚合 `ConfigCenterClient` /
//! `RegistryCenterClient` 两个 trait，按 `MicroSvcConfig` 选中的后端
//! （Consul / Etcd / Nacos）创建对应客户端，并提供配置拉取、变更订阅、
//! 服务注册/注销/发现及本地快照回退等能力。

mod consul;
mod etcd;
mod hub_client_error;
mod hub_client;
mod nacos;
/// Hub 客户端通用配置（被各后端配置内嵌复用）。
pub mod hub_client_config;

pub use consul::*;
pub use etcd::*;
pub use hub_client_error::*;
pub use hub_client::*;
pub use nacos::*;
