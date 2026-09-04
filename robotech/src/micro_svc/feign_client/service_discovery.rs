use crate::micro_svc::hub_client::discover_service;
use crate::micro_svc::ServiceInstance;
use arc_swap::ArcSwap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{info, warn};

pub struct ServiceDiscovery {
    svc_name: String,
    instances: ArcSwap<Vec<ServiceInstance>>,
    refresh_interval: Duration,
    _refresh_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ServiceDiscovery {
    pub fn new(svc_name: &str, refresh_interval: Duration) -> Self {
        Self {
            svc_name: svc_name.to_string(),
            instances: ArcSwap::from_pointee(Vec::new()),
            refresh_interval,
            _refresh_handle: Mutex::new(None),
        }
    }

    pub async fn init(&self) -> Result<(), crate::cfg::CfgError> {
        let instances = discover_service(&self.svc_name).await?;
        info!(
            "service discovery: {} found {} instances",
            self.svc_name,
            instances.len()
        );
        self.instances.store(Arc::new(instances));
        Ok(())
    }

    pub fn get_instances(&self) -> Arc<Vec<ServiceInstance>> {
        self.instances.load_full()
    }

    pub fn start_refresh_loop(self: &Arc<Self>) {
        let this = Arc::clone(self);
        let svc_name = self.svc_name.clone();
        let interval = self.refresh_interval;
        let join_handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                match discover_service(&svc_name).await {
                    Ok(instances) => {
                        info!(
                            "service discovery: {} refreshed {} instances",
                            svc_name,
                            instances.len()
                        );
                        this.instances.store(Arc::new(instances));
                    }
                    Err(e) => {
                        warn!("service discovery: {} refresh failed: {:?}", svc_name, e);
                    }
                }
            }
        });
        *self._refresh_handle.lock().unwrap() = Some(join_handle);
    }
}

impl Drop for ServiceDiscovery {
    fn drop(&mut self) {
        if let Some(handle) = self._refresh_handle.lock().unwrap().take() {
            handle.abort();
        }
    }
}