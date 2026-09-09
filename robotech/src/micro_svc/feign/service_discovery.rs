use crate::micro_svc::hub_client::discover_service;
use crate::micro_svc::ServiceInstance;
use arc_swap::ArcSwap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// 服务发现器：负责拉取并缓存指定服务的最新实例列表。
///
/// 实例列表存放在 `ArcSwap` 中，可无锁并发读取；`init` 完成首次加载，
/// `start_refresh_loop` 按 `refresh_interval` 周期刷新。
pub struct ServiceDiscovery {
    svc_name: String,
    instances: ArcSwap<Vec<ServiceInstance>>,
    refresh_interval: Duration,
    _refresh_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ServiceDiscovery {
    /// 创建服务发现器（此时尚未加载任何实例，需先调用 `init`）。
    pub fn new(svc_name: &str, refresh_interval: Duration) -> Self {
        Self {
            svc_name: svc_name.to_string(),
            instances: ArcSwap::from_pointee(Vec::new()),
            refresh_interval,
            _refresh_handle: Mutex::new(None),
        }
    }

    /// 首次加载实例列表；失败时返回错误。
    ///
    /// ## 错误
    /// 后端注册中心不可用或未配置时返回 `CfgError`。
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

    /// 返回当前缓存的实例列表快照。
    pub fn get_instances(&self) -> Arc<Vec<ServiceInstance>> {
        self.instances.load_full()
    }

    /// 启动后台刷新任务，按 `refresh_interval` 周期重新拉取实例列表。
    ///
    /// 刷新失败仅记录告警，不中断循环；任务句柄保存在内部，
    /// `ServiceDiscovery` 被 drop 时自动终止。
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