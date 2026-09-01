use crate::micro_svc::registry_center::{RegistryCenterError, ServiceInstance};
use async_trait::async_trait;

/// 所有后端适配器（etcd / Consul / Nacos）必须实现的统一契约。
///
/// 门面层 `RegistryCenter` 和业务代码只依赖这个 trait，完全不感知具体是哪个后端——
/// 这是整个库"配置驱动切换、开发者无感知"的核心抽象点。新增一个后端，
/// 只需要在 `micro_svc/hub_client/` 下新增一个模块实现这个 trait，不需要动其它任何代码。
#[async_trait]
pub trait RegistryCenterClient: Send + Sync {
    /// 后端名称，仅用于日志/错误信息展示。
    fn name(&self) -> &'static str;

    /// 注册一个服务实例。
    async fn register(&self, instance: &ServiceInstance) -> Result<(), RegistryCenterError>;

    /// 注销一个服务实例。
    async fn deregister(&self, instance_id: &str) -> Result<(), RegistryCenterError>;

    /// 发现指定服务的所有健康实例。
    async fn discover(
        &self,
        service_name: &str,
    ) -> Result<Vec<ServiceInstance>, RegistryCenterError>;
}
