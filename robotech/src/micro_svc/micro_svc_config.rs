#[cfg(any(feature = "config-center", feature = "registry-center"))]
use crate::micro_svc::{ConsulConfig, EtcdConfig, NacosConfig};
use serde::{Deserialize, Serialize};

pub const MICRO_SVC_CONFIG_KEY: &str = "micro-svc";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MicroSvcConfig {
    /// 服务名称
    #[serde(default)]
    pub svc_name: Option<String>,
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    #[serde(default)]
    pub consul: Option<ConsulConfig>,
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    #[serde(default)]
    pub etcd: Option<EtcdConfig>,
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    #[serde(default)]
    pub nacos: Option<NacosConfig>,
}
