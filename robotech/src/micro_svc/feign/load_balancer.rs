use crate::micro_svc::ServiceInstance;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 负载均衡策略：从可用实例列表中选取一个实例。
///
/// 实现需要满足 `Send + Sync`，可安全地在并发请求间共享。
pub trait LoadBalancer: Send + Sync {
    /// 从实例列表中选取一个实例；列表为空时返回 `None`。
    fn choose(&self, instances: &[ServiceInstance]) -> Option<ServiceInstance>;
}

/// 轮询负载均衡器：按原子计数器依次轮流选择实例，保证请求均匀分布。
pub struct RoundRobinBalancer {
    counter: AtomicUsize,
}

impl RoundRobinBalancer {
    /// 创建一个初始计数器为 0 的轮询负载均衡器。
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl Default for RoundRobinBalancer {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for RoundRobinBalancer {
    fn choose(&self, instances: &[ServiceInstance]) -> Option<ServiceInstance> {
        if instances.is_empty() {
            return None;
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % instances.len();
        Some(instances[idx].clone())
    }
}