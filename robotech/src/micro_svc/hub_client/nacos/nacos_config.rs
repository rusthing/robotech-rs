use crate::micro_svc::{ConfigCenterConfig, RegistryCenterConfig};
use serde::{Deserialize, Serialize};
use crate::micro_svc::hub_client_config::HubClientConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct NacosConfig {
    /// Hub客户端配置
    #[serde(flatten)]
    pub hub_client: HubClientConfig,

    /// 配置中心
    #[cfg(feature = "config-center")]
    #[serde(default)]
    pub config: Option<ConfigCenterConfig>,
    /// 注册中心
    #[cfg(feature = "registry-center")]
    #[serde(default)]
    pub registry: Option<RegistryCenterConfig>,
}
