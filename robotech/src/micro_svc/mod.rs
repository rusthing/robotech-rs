//! # 微服务模块
//!
//! 面向微服务场景提供配置中心（config_center）、注册中心（registry_center）、
//! Hub 客户端门面（hub_client）与 Feign HTTP 客户端（feign）等能力。
//! 通过 `MicroSvcConfig` 选择 Consul / Etcd / Nacos 后端，由配置文件驱动切换，
//! 业务代码只依赖本模块统一暴露的 trait 与结构体，不感知具体后端。
//!
//! ## feature 分层
//!
//! - `micro-svc-client`：契约（两个 trait + EO + 错误）与三后端实现，**不依赖 web**；
//!   网关、MQ 消费者等非 axum 进程复用服务发现/配置拉取时只开它。
//! - `config-center`：在 `micro-svc-client` 之上追加 TOML 注入能力。
//! - `registry-center`：在 `micro-svc-client` 之上追加 web（健康检查 URL/监听端口上报）
//!   与 Feign 共享选址层。

#[cfg(feature = "micro-svc-client")]
mod config_center;
// `ServiceDiscovery` / `LoadBalancer` 是共享选址层的基础设施，只要有注册中心就该可用；
// 只有 `FeignApiClient` 本身才需要 `api-client` feature（见 feign/mod.rs）。
// `feign` = `api-client` + `registry-center`，所以这里等价于 `registry-center`。
#[cfg(feature = "registry-center")]
mod feign;
#[cfg(feature = "micro-svc-client")]
mod hub_client;
#[cfg(feature = "registry-center")]
mod instance_resolver;
mod micro_svc_config;
#[cfg(feature = "micro-svc-client")]
mod registry_center;

#[cfg(feature = "micro-svc-client")]
pub use config_center::*;
#[cfg(feature = "registry-center")]
pub use feign::*;
#[cfg(feature = "micro-svc-client")]
pub use hub_client::*;
#[cfg(feature = "registry-center")]
pub use instance_resolver::*;
pub use micro_svc_config::*;
#[cfg(feature = "micro-svc-client")]
pub use registry_center::*;
