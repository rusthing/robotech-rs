//! # Feign 模块
//!
//! 提供服务发现 + 负载均衡 + 失败熔断（冷却）的声明式 HTTP 客户端 `FeignApiClient`
//! （需启用 `api-client` feature），以及负载均衡（`LoadBalancer`）与服务发现
//! （`ServiceDiscovery`）等基础设施。微服务模式下自动发现目标服务的实例列表并
//! 轮询选择可用实例发起请求，失败实例按阈值进入冷却期；简单模式下直接指向固定 base_url。
//!
//! 「服务名 → 实例快照」这件事统一由 `micro_svc::instance_resolver` 持有：Feign 与其它
//! 动态调用方共享同一份缓存，不再每个客户端各起一个后台刷新任务。`ServiceDiscovery`
//! 的 `start_refresh_loop()` 只留给不经过共享层的直接使用场景。

mod load_balancer;
mod service_discovery;

pub use load_balancer::*;
pub use service_discovery::*;

#[cfg(feature = "api-client")]
mod feign_api_client;

#[cfg(feature = "api-client")]
pub use feign_api_client::*;
