use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 服务实例的完整信息，用于注册与发现。
///
/// 各后端 (etcd / Consul / Nacos) 的适配器会把本结构体转换成各自的原生格式
/// （见对应 backend 模块里的实现），上层业务代码只需要认识这一个结构体。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ServiceInstance {
    pub instance_id: String,
    pub service_name: String,
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
            self.service_name, self.ip, self.port, self.instance_id
        )
    }
}
