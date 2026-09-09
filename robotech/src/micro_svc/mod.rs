//! # 微服务模块
//!
//! 面向微服务场景提供配置中心（config_center）、注册中心（registry_center）、
//! Hub 客户端门面（hub_client）与 Feign HTTP 客户端（feign）等能力。
//! 通过 `MicroSvcConfig` 选择 Consul / Etcd / Nacos 后端，由配置文件驱动切换，
//! 业务代码只依赖本模块统一暴露的 trait 与结构体，不感知具体后端。

#[cfg(any(feature = "config-center", feature = "registry-center"))]
mod config_center;
#[cfg(feature = "feign")]
mod feign;
#[cfg(any(feature = "config-center", feature = "registry-center"))]
mod hub_client;
mod micro_svc_config;
#[cfg(feature = "registry-center")]
mod registry_center;

#[cfg(any(feature = "config-center", feature = "registry-center"))]
pub use config_center::*;
#[cfg(feature = "feign")]
pub use feign::*;
#[cfg(any(feature = "config-center", feature = "registry-center"))]
pub use hub_client::*;
pub use micro_svc_config::*;
#[cfg(feature = "registry-center")]
pub use registry_center::*;
