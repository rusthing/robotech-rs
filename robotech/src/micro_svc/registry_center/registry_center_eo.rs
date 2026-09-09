use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 注册中心的定位信息：命名空间 + 分组 + 服务名。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegistryKey {
    /// 命名空间(Nacos有此概念，Consul / etcd 无用)
    pub namespace: Option<String>,
    /// 分组(Nacos有此概念，Consul / etcd 无用)
    /// 一般用环境来分组，例如 `dev`、`test`、`prod`等
    pub group: Option<String>,
    /// 服务名
    pub svc_name: String,
}

/// 服务实例的完整信息，用于注册与发现。
///
/// 各后端 (etcd / Consul / Nacos) 的适配器会把本结构体转换成各自的原生格式
/// （见对应 backend 模块里的实现），上层业务代码只需要认识这一个结构体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ServiceInstance {
    /// 命名空间（Nacos 有此概念，Consul / etcd 无用）
    pub namespace: Option<String>,
    /// 分组（Nacos 有此概念，Consul / etcd 无用）
    pub group: Option<String>,
    /// 服务名
    pub svc_name: String,
    /// 实例唯一 ID（形如 `{svc_name}-{ip}-{port}`，IP 中的点替换为短横线）
    pub instance_id: String,
    /// 实例 IP 地址
    pub ip: String,
    /// 实例端口
    pub port: u16,
    /// 实例附加元数据（各后端按需透传）
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// 健康检查 URL（可选）
    pub health_check_url: Option<String>,
}

impl std::fmt::Display for ServiceInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}@{}:{} (id={})",
            self.svc_name, self.ip, self.port, self.instance_id
        )
    }
}
