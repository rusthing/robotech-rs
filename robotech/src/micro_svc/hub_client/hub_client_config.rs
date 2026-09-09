use serde::{Deserialize, Serialize};
use wheel_rs::serde::vec_serde;

/// Hub 客户端通用配置：服务器地址列表、命名空间与分组。
///
/// 被各后端配置（Consul / Etcd / Nacos）通过 `#[serde(flatten)]` 内嵌复用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct HubClientConfig {
    /// 服务器基础 URL 列表
    #[serde(with = "vec_serde")]
    pub base_url: Vec<String>,
    /// 命名空间（Nacos 原生支持；Consul / etcd 作为 key 前缀）
    #[serde(default)]
    pub namespace: Option<String>,
    /// 分组（一般用环境，如 `dev` / `prod`；为空时回退到 profile）
    #[serde(default)]
    pub group: Option<String>,
}
