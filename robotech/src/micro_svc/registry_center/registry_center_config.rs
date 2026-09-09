use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wheel_rs::serde::{duration_serde, ipnet_option_serde};

/// 注册中心的本地配置：注册/续报周期与实例 IP 子网。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RegistryCenterConfig {
    /// 注册失败后的重试间隔
    #[serde(with = "duration_serde", default = "retry_interval_default")]
    pub retry_interval: Duration,
    /// 注册成功后的刷新（续报）间隔
    #[serde(with = "duration_serde", default = "refresh_interval_default")]
    pub refresh_interval: Duration,
    /// 用于选择本机 IP 的子网（CIDR），为空时自动选择默认网卡 IP
    #[serde(with = "ipnet_option_serde", default)]
    pub sub_net: Option<IpNet>,
}

impl Default for RegistryCenterConfig {
    fn default() -> Self {
        Self {
            retry_interval: retry_interval_default(),
            refresh_interval: refresh_interval_default(),
            sub_net: None,
        }
    }
}

fn retry_interval_default() -> Duration {
    Duration::from_secs(3)
}
fn refresh_interval_default() -> Duration {
    Duration::from_secs(30)
}
