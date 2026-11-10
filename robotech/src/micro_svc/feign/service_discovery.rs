use crate::micro_svc::hub_client::discover_service;
use crate::micro_svc::ServiceInstance;
use arc_swap::ArcSwap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// 服务发现器：负责拉取并缓存指定服务的最新实例列表。
///
/// 实例列表存放在 `ArcSwap` 中，可无锁并发读取；`init` 完成首次加载，
/// `start_refresh_loop` 按 `refresh_interval` 周期刷新。
pub struct ServiceDiscovery {
    svc_name: String,
    namespace: Option<String>,
    group: Option<String>,
    instances: ArcSwap<Vec<ServiceInstance>>,
    refresh_interval: Duration,
    _refresh_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ServiceDiscovery {
    /// 创建服务发现器（此时尚未加载任何实例，需先调用 `init`）。
    pub fn new(svc_name: &str, refresh_interval: Duration) -> Self {
        Self {
            svc_name: svc_name.to_string(),
            namespace: None,
            group: None,
            instances: ArcSwap::from_pointee(Vec::new()),
            refresh_interval,
            _refresh_handle: Mutex::new(None),
        }
    }

    /// 创建带指定 namespace/group 的服务发现器。
    ///
    /// 传入的 namespace/group 会覆盖 HubClient 自身的 namespace/group，
    /// 用于发现其他环境或命名空间中的服务。
    pub fn with_namespace_and_group(
        svc_name: &str,
        refresh_interval: Duration,
        namespace: Option<String>,
        group: Option<String>,
    ) -> Self {
        Self {
            svc_name: svc_name.to_string(),
            namespace,
            group,
            instances: ArcSwap::from_pointee(Vec::new()),
            refresh_interval,
            _refresh_handle: Mutex::new(None),
        }
    }

    /// 立即重新拉取一次实例列表，原子替换缓存并返回新快照。
    ///
    /// 供 `init`、后台刷新循环，以及「缓存为空时来了真实请求」三条路径共用；
    /// 失败时保留旧列表（不清空），避免一次注册中心抖动把好实例全抹掉。
    ///
    /// ## 错误
    /// 后端注册中心不可用或未配置时返回 `CfgError`。
    pub async fn refresh(&self) -> Result<Arc<Vec<ServiceInstance>>, crate::cfg::CfgError> {
        let instances =
            discover_service(&self.svc_name, self.namespace.clone(), self.group.clone()).await?;
        info!(
            "service discovery: {} found {} instances",
            self.svc_name,
            instances.len()
        );
        let snapshot = Arc::new(instances);
        self.instances.store(Arc::clone(&snapshot));
        Ok(snapshot)
    }

    /// 首次加载实例列表；失败时返回错误。
    ///
    /// ## 错误
    /// 后端注册中心不可用或未配置时返回 `CfgError`。
    pub async fn init(&self) -> Result<(), crate::cfg::CfgError> {
        self.refresh().await.map(|_| ())
    }

    /// 注册中心里查询用的服务名（不含 namespace/group 前缀），仅用于日志与错误信息。
    pub fn svc_name(&self) -> &str {
        &self.svc_name
    }

    /// 返回当前缓存的实例列表快照。
    pub fn get_instances(&self) -> Arc<Vec<ServiceInstance>> {
        self.instances.load_full()
    }

    /// 启动后台刷新任务，按 `refresh_interval` 周期重新拉取实例列表。
    ///
    /// 只持有 [`Weak`]：配置热更新重建 API 客户端后，旧客户端一旦被释放，
    /// 任务在下个周期自行退出。用 `Arc` 的话任务会自己把 `ServiceDiscovery`
    /// 撑住，`Drop` 里的 `abort()` 永远没机会执行——生产上表现为改对了服务名，
    /// 日志里旧名字的「refreshed 0 instances」还每 30 秒刷一次，永不停止。
    ///
    /// 刷新失败仅记录告警，不中断循环；任务句柄保存在内部，
    /// `ServiceDiscovery` 被 drop 时自动终止。
    pub fn start_refresh_loop(self: &Arc<Self>) {
        let weak: Weak<Self> = Arc::downgrade(self);
        let svc_name = self.svc_name.clone();
        let interval = self.refresh_interval;
        let join_handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                // 每轮只在 refresh 期间短暂持有强引用，睡眠期间不占着客户端
                let Some(this) = weak.upgrade() else {
                    info!(
                        "service discovery: {} client released, refresh loop stopped",
                        svc_name
                    );
                    break;
                };
                if let Err(e) = this.refresh().await {
                    warn!("service discovery: {} refresh failed: {:?}", svc_name, e);
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