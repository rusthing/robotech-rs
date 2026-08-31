use crate::micro_svc::config_center::{ConfigCenterError, ConfigItem};
use crate::micro_svc::ConfigKey;
use async_trait::async_trait;
use tokio::sync::watch;

/// 所有后端适配器（etcd / Consul / Nacos）必须实现的统一契约。
///
/// 门面层 `ConfigCenter` 和业务代码只依赖这个 trait，完全不感知具体是哪个后端——
/// 这是整个库"配置驱动切换、开发者无感知"的核心抽象点。新增一个后端，
/// 只需要在 `micro_svc/hub_client/` 下新增一个模块实现这个 trait，不需要动其它任何代码。
#[async_trait]
pub trait ConfigCenterClient: Send + Sync {
    /// 后端名称，仅用于日志/错误信息展示。
    fn name(&self) -> &'static str;

    fn config_key(&self) -> Result<ConfigKey, ConfigCenterError>;

    /// 按指定的 key 拉取一次配置的当前内容。
    async fn fetch(&self, key: &ConfigKey) -> Result<ConfigItem, ConfigCenterError>;

    /// 订阅某个 key 的变更，返回一个事件接收端。
    /// etcd 的 watch、Consul 的 blocking query、Nacos 的长连接推送，这三种完全不同的
    /// 变更感知机制，在各自的实现内部被吸收掉，对上层统一表现为同一种 channel 事件流。
    async fn watch(
        &self,
        key: &ConfigKey,
        config_changed_sender: watch::Sender<()>,
    ) -> Result<(), ConfigCenterError>;
}