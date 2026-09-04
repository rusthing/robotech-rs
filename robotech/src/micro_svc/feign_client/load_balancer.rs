use crate::micro_svc::ServiceInstance;
use std::sync::atomic::{AtomicUsize, Ordering};

pub trait LoadBalancer: Send + Sync {
    fn choose(&self, instances: &[ServiceInstance]) -> Option<ServiceInstance>;
}

pub struct RoundRobinBalancer {
    counter: AtomicUsize,
}

impl RoundRobinBalancer {
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