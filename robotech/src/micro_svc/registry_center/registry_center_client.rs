use crate::micro_svc::registry_center::{RegistryCenterError, ServiceInstance};
use async_trait::async_trait;

/// 所有后端适配器（etcd / Consul / Nacos）必须实现的统一契约。
///
/// 门面层 `HubClient` 和业务代码只依赖这个 trait，完全不感知具体是哪个后端——
/// 这是整个库"配置驱动切换、开发者无感知"的核心抽象点。新增一个后端，
/// 只需要在 `micro_svc/hub_client/` 下新增一个模块实现这个 trait，不需要动其它任何代码。
#[async_trait]
pub trait RegistryCenterClient: Send + Sync {
    /// 后端名称，仅用于日志/错误信息展示。
    fn name(&self) -> &'static str;

    /// 注册一个服务实例。
    async fn register(&self, service_instance: &ServiceInstance)
    -> Result<(), RegistryCenterError>;

    /// 注销一个服务实例。
    async fn deregister(
        &self,
        service_instance: &ServiceInstance,
    ) -> Result<(), RegistryCenterError>;

    /// 发现指定服务的所有健康实例。
    async fn discover(
        &self,
        namespace: Option<String>,
        group: Option<String>,
        svc_name: &str,
    ) -> Result<Vec<ServiceInstance>, RegistryCenterError>;

    /// 列出指定 namespace / group 下的全部服务名。
    ///
    /// 后端不支持枚举时返回空列表，调用方降级为手输（默认实现即返回 `Ok(vec![])`，
    /// 仅支持枚举的后端——目前是 Consul——需要 override 此方法）。
    async fn list_services(
        &self,
        namespace: Option<String>,
        group: Option<String>,
    ) -> Result<Vec<String>, RegistryCenterError> {
        let _ = (namespace, group);
        Ok(vec![])
    }
}
