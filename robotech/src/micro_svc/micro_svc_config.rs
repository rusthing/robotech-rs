#[cfg(any(feature = "config-center", feature = "registry-center"))]
use crate::micro_svc::{ConsulConfig, EtcdConfig, NacosConfig};
use serde::{Deserialize, Serialize};

/// 微服务配置在配置文件/配置中心中使用的键名（`micro-svc`）。
pub const MICRO_SVC_CONFIG_KEY: &str = "micro-svc";

/// 微服务全局配置：服务名、环境（profile）与各后端（Consul / Etcd / Nacos）配置。
///
/// 通常以 `micro-svc` 为键从配置文件中反序列化得到；启用对应 feature 后，
/// 配置中心/注册中心会依据其中选中的后端创建相应的客户端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MicroSvcConfig {
    /// 服务名称
    #[serde(default)]
    pub svc_name: Option<String>,
    /// 环境(dev/test/prod)
    #[serde(default)]
    pub profile: Option<String>,
    /// Consul 后端配置（需启用 `config-center` 或 `registry-center` feature）
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    #[serde(default)]
    pub consul: Option<ConsulConfig>,
    /// Etcd 后端配置（需启用 `config-center` 或 `registry-center` feature）
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    #[serde(default)]
    pub etcd: Option<EtcdConfig>,
    /// Nacos 后端配置（需启用 `config-center` 或 `registry-center` feature）
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    #[serde(default)]
    pub nacos: Option<NacosConfig>,
    /// 刷新作用域在配置中心使用的 data_id。
    ///
    /// 默认值为 `"refresh-scope.toml"`。该 key 会自动纳入配置中心的监听与拉取列表。
    /// 业务层通过 [`set_refresh_scope`] 精确更新某个字段的值，不冲掉其它字段。
    /// 设置为 `None` 可禁用刷新作用域功能。
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    #[serde(default = "refresh_scope_data_id_default")]
    pub refresh_scope: String,
}

impl Default for MicroSvcConfig {
    fn default() -> Self {
        Self {
            svc_name: None,
            profile: None,
            #[cfg(any(feature = "config-center", feature = "registry-center"))]
            consul: None,
            #[cfg(any(feature = "config-center", feature = "registry-center"))]
            etcd: None,
            #[cfg(any(feature = "config-center", feature = "registry-center"))]
            nacos: None,
            #[cfg(any(feature = "config-center", feature = "registry-center"))]
            refresh_scope: refresh_scope_data_id_default(),
        }
    }
}

/// 刷新作用域 data_id 的默认值。
#[cfg(any(feature = "config-center", feature = "registry-center"))]
fn refresh_scope_data_id_default() -> String {
    "refresh-scope.toml".to_string()
}
