#[cfg(any(feature = "config-center", feature = "registry-center"))]
use crate::micro_svc::{ConsulConfig, EtcdConfig, NacosConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// 缓存版本 key 映射
    ///
    /// key = changed HashMap 中用于 [`has_config_changed`] 匹配的本地 key 名；
    /// value = 配置中心中对应的 key 名（data_id，需带扩展名如 `.json`）。
    ///
    /// 配置中心 key 的值发生变化时，框架会将其注入到配置的
    /// `micro-svc.cache-keys.{key}` 路径，`diff_config` 自然检测到，
    /// 然后通过 `setup(changed)` 回调通知业务层的 `setup_xxx` 方法。
    ///
    /// 值没有时写当前时间戳，改变时也写当前时间戳即可。
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    #[serde(default)]
    pub cache_keys: HashMap<String, String>,
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
            cache_keys: HashMap::new(),
        }
    }
}