use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ServiceInstance {
    pub group: Option<String>,
    pub svc_name: String,
    pub instance_id: String,
    pub ip: String,
    pub port: u16,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
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
