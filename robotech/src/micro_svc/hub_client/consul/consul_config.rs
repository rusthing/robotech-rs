use crate::micro_svc::hub_client_config::HubClientConfig;
use crate::micro_svc::{ConfigCenterConfig, RegistryCenterConfig};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wheel_rs::serde::duration_serde;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConsulConfig {
    /// Hub客户端配置
    #[serde(flatten)]
    pub hub_client: HubClientConfig,

    /// Consul查询超时时间
    /// 正常查询（不带 index 参数），Consul 会立即返回当前值 + 一个 X-Consul-Index 响应头（也叫 ModifyIndex）
    /// 阻塞查询（带 index + wait）, Consul 收到这个请求后会：
    /// 1. 对比当前 key 的 ModifyIndex 是否已经大于 index 参数的值
    /// 2. 如果已变化 → 立即返回新值
    /// 3. 如果没变化 → hold 住 HTTP 连接，最长等待 wait 参数的值（即 blocking_query_timeout），期间 key 一旦变化就立即返回
    /// 4. 如果 10s 超时了还没变 → 返回和之前相同的响应
    #[serde(with = "duration_serde", default = "blocking_query_timeout_default")]
    pub blocking_query_timeout: Duration,

    /// 配置中心
    #[cfg(feature = "config-center")]
    #[serde(default)]
    pub config: Option<ConfigCenterConfig>,
    /// 注册中心
    #[cfg(feature = "registry-center")]
    #[serde(default)]
    pub registry: Option<RegistryCenterConfig>,
}

fn blocking_query_timeout_default() -> Duration {
    Duration::from_secs(10)
}
