use crate::micro_svc::hub_client_config::HubClientConfig;
use crate::micro_svc::ConfigCenterConfig;
#[cfg(feature = "registry-center")]
use crate::micro_svc::RegistryCenterConfig;
use serde::{Deserialize, Serialize};

/// Nacos 后端配置：Hub 客户端通用配置 + 配置/注册中心开关。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct NacosConfig {
    /// Hub客户端配置
    #[serde(flatten)]
    pub hub_client: HubClientConfig,

    /// 配置中心
    #[serde(default)]
    pub config: Option<ConfigCenterConfig>,
    /// 注册中心
    #[cfg(feature = "registry-center")]
    #[serde(default)]
    pub registry: Option<RegistryCenterConfig>,
}
