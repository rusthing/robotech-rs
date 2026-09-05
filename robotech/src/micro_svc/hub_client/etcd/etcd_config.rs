use crate::micro_svc::etcd_connect_option_wrapper::EtcdConnectOptionsWrapper;
use crate::micro_svc::hub_client_config::HubClientConfig;
use crate::micro_svc::ConfigCenterConfig;
#[cfg(feature = "registry-center")]
use crate::micro_svc::RegistryCenterConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct EtcdConfig {
    /// Hub客户端配置
    #[serde(flatten)]
    pub hub_client: HubClientConfig,

    /// etcd 连接选项
    #[serde(default)]
    pub connect_options: EtcdConnectOptionsWrapper,

    /// 配置中心
    #[serde(default)]
    pub config: Option<ConfigCenterConfig>,
    /// 注册中心
    #[cfg(feature = "registry-center")]
    #[serde(default)]
    pub registry: Option<RegistryCenterConfig>,
}
