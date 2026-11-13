//! # 共享实例选址
//!
//! 框架里「服务名 → 实例」只有这一处实现。[`InstanceResolver`] 按
//! `(服务名, namespace, group)` 在进程内共享一份实例快照；调用方只决定策略
//! （缓存多久、零实例算不算错、要不要重试），不必各自再写一份 HashMap + 计数器。
//!
//! ## 为什么下沉的是「选址」，不是整个 Feign 客户端
//!
//! `FeignApiClient` 把选址和**发请求**打包在一起：`Ro<E>` 响应、非 2xx 即错误、
//! 失败自动换实例重试、服务名由 `[api.xxx]` 在启动期静态装配。这套契约属于
//! 「调用一个已知服务」，不适合「运行时才知道服务名、要原始 status/body、
//! 一次投递不许客户端偷偷重试」的调用方。所以复用边界划在**选址**：
//! Feign 自己、syn 的 HTTP 投递、以及将来的动态 webhook 都走本模块。
//!
//! ## 缓存与刷新
//!
//! - **懒刷新**：只有请求到来且缓存超过 `ttl` 才查注册中心，没有常驻后台任务，
//!   也就没有「旧客户端被释放后刷新任务还在刷」的生命周期问题。
//! - **空结果负缓存**：下游整体下线时按 `ttl` 缓存空列表，不会每条记录都打一次
//!   consul；实例恢复后最多 `ttl` 自动生效，不需要重启。
//! - **失败节流 + 保留旧快照**：查询失败按 `min(ttl, 1s)` 节流并保留上次的好实例，
//!   注册中心抖一下不会把可用实例全抹掉。
//! - **并发合并**：同一服务同时只放一个刷新请求出去，其余请求读当前快照。
//! - **每服务一个轮转游标**，不同服务之间不串号。

use crate::micro_svc::hub_client::discover_service;
use crate::micro_svc::{LoadBalancer, RoundRobinBalancer, ServiceDiscovery, ServiceInstance};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;
use tracing::warn;

/// 查询失败后的最大抑制时长。成功/空结果按调用方给的 `ttl`，失败单独缩短，
/// 既避免打爆注册中心，也别让一次抖动拖成半个 `ttl` 不可用。
const ERROR_RETRY_AFTER: Duration = Duration::from_secs(1);

/// 一次选址的结论。
#[derive(Debug, Clone)]
pub struct InstanceSnapshot {
    /// 当前可用实例（刷新失败时是**旧快照**，可能为空）
    pub instances: Arc<Vec<ServiceInstance>>,
    /// 最近一次刷新失败的原因；`None` 表示成功或缓存还新鲜
    pub last_error: Option<String>,
}

/// 选址失败。区分「部署问题」与「注册中心问题」，调用方据此决定致命还是可重试。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// 服务名为空：配置错误，不该去查注册中心
    InvalidServiceName,
    /// 注册中心里该服务没有健康实例（下游整体下线）
    NoHealthyInstance { svc: String },
    /// 注册中心不可用，且没有旧快照可用
    RegistryUnavailable { svc: String, detail: String },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidServiceName => write!(f, "服务名为空"),
            Self::NoHealthyInstance { svc } => {
                write!(f, "注册中心里服务 {svc} 没有可用实例")
            }
            Self::RegistryUnavailable { svc, detail } => {
                write!(f, "从注册中心发现服务 {svc} 失败：{detail}")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// 最近一次刷新的时间与结果。用 `Instant` 而不是系统时钟，避免改机器时间影响 TTL。
#[derive(Debug, Clone)]
struct Attempt {
    at: Instant,
    error: Option<String>,
}

impl Attempt {
    fn effective_ttl(&self, ttl: Duration) -> Duration {
        if self.error.is_some() {
            ttl.min(ERROR_RETRY_AFTER)
        } else {
            ttl
        }
    }

    fn is_fresh(&self, ttl: Duration, now: Instant) -> bool {
        now.saturating_duration_since(self.at) <= self.effective_ttl(ttl)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResolverKey {
    svc: String,
    namespace: Option<String>,
    group: Option<String>,
}

/// 一个服务的共享选址状态。由 [`InstanceResolver`] 持有，进程内每个服务一份。
pub struct ResolverEntry {
    svc_name: String,
    namespace: Option<String>,
    group: Option<String>,
    discovery: Arc<ServiceDiscovery>,
    balancer: RoundRobinBalancer,
    last_attempt: Mutex<Option<Attempt>>,
    /// 只保护「刷新」这段异步过程，读取快照走 `ArcSwap`，不进这把锁
    refresh_lock: AsyncMutex<()>,
}

impl ResolverEntry {
    /// 注册中心里查询用的服务名（不含 namespace/group 前缀），用于日志与报错。
    pub fn svc_name(&self) -> &str {
        &self.svc_name
    }

    /// 内部的服务发现器，供需要直接读快照的调用方使用。
    pub fn discovery(&self) -> &Arc<ServiceDiscovery> {
        &self.discovery
    }

    fn last(&self) -> Option<Attempt> {
        self.last_attempt
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 仍在有效期内的最近一次尝试；空结果（负缓存）也算有效。
    fn fresh_last(&self, ttl: Duration) -> Option<Attempt> {
        self.last()
            .filter(|attempt| attempt.is_fresh(ttl, Instant::now()))
    }

    fn record(&self, attempt: Attempt) {
        *self.last_attempt.lock().unwrap_or_else(|e| e.into_inner()) = Some(attempt);
    }

    fn current_snapshot(&self, error: Option<String>) -> InstanceSnapshot {
        InstanceSnapshot {
            instances: self.discovery.get_instances(),
            last_error: error,
        }
    }

    /// 取当前实例快照，必要时刷新一次。
    ///
    /// `fetch` 由调用方注入，便于在单测里替掉注册中心；生产路径用 [`Self::snapshot`]。
    /// 无论成功失败都**不会**返回 `Err`：失败时 `instances` 是旧快照，
    /// 原因放在 `last_error`，由调用方按自己的语义决定怎么处理。
    pub async fn snapshot_with<F, Fut>(&self, ttl: Duration, fetch: F) -> InstanceSnapshot
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Vec<ServiceInstance>, String>>,
    {
        if let Some(attempt) = self.fresh_last(ttl) {
            return self.current_snapshot(attempt.error);
        }

        let _guard = self.refresh_lock.lock().await;
        // 等锁期间可能已经被别的请求刷过了，别重复查
        if let Some(attempt) = self.fresh_last(ttl) {
            return self.current_snapshot(attempt.error);
        }

        let result = self.discovery.refresh_with(fetch).await;
        let error = result.as_ref().err().cloned();
        self.record(Attempt {
            at: Instant::now(),
            error: error.clone(),
        });
        match result {
            Ok(snapshot) => InstanceSnapshot {
                instances: snapshot,
                last_error: None,
            },
            Err(detail) => {
                warn!(
                    "service discovery: {} refresh failed: {}（保留旧快照）",
                    self.svc_name, detail
                );
                self.current_snapshot(Some(detail))
            }
        }
    }

    /// 按 `ttl` 取当前实例快照，过期才查注册中心。
    pub async fn snapshot(&self, ttl: Duration) -> InstanceSnapshot {
        let svc = self.svc_name.clone();
        let namespace = self.namespace.clone();
        let group = self.group.clone();
        self.snapshot_with(ttl, || async move {
            discover_service(&svc, namespace, group)
                .await
                .map(|instances| {
                    instances
                        .into_iter()
                        .filter(|i| !i.ip.trim().is_empty() && i.port > 0)
                        .collect()
                })
                .map_err(|e| e.to_string())
        })
        .await
    }
}

/// 进程级共享的选址表。
#[derive(Default)]
pub struct InstanceResolver {
    entries: Mutex<HashMap<ResolverKey, Arc<ResolverEntry>>>,
}

impl InstanceResolver {
    /// 取（必要时创建）一个服务的共享选址状态。
    ///
    /// `refresh_interval` 只作为 `ServiceDiscovery` 的元数据保留（直接调用
    /// `start_refresh_loop()` 的旧用法）；共享路径的刷新节奏由 `snapshot(ttl)` 决定。
    pub fn entry(
        &self,
        svc_name: &str,
        namespace: Option<String>,
        group: Option<String>,
        refresh_interval: Duration,
    ) -> Arc<ResolverEntry> {
        let key = ResolverKey {
            svc: svc_name.to_string(),
            namespace: namespace.clone(),
            group: group.clone(),
        };
        let mut guard = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .entry(key.clone())
            .or_insert_with(|| {
                Arc::new(ResolverEntry {
                    svc_name: key.svc.clone(),
                    namespace: key.namespace.clone(),
                    group: key.group.clone(),
                    discovery: Arc::new(ServiceDiscovery::with_namespace_and_group(
                        svc_name,
                        refresh_interval,
                        namespace,
                        group,
                    )),
                    balancer: RoundRobinBalancer::new(),
                    last_attempt: Mutex::new(None),
                    refresh_lock: AsyncMutex::new(()),
                })
            })
            .clone()
    }
}

/// 进程级共享选址表。同一服务名在 Feign 客户端与动态调用方之间是同一份快照。
pub fn resolver() -> &'static InstanceResolver {
    static RESOLVER: OnceLock<InstanceResolver> = OnceLock::new();
    RESOLVER.get_or_init(InstanceResolver::default)
}

/// 按服务名选一个健康实例。这是「只想要一个地址、自己发请求」的标准入口。
pub async fn choose_instance(
    svc_name: &str,
    namespace: Option<String>,
    group: Option<String>,
    ttl: Duration,
) -> Result<ServiceInstance, ResolveError> {
    if svc_name.trim().is_empty() {
        return Err(ResolveError::InvalidServiceName);
    }
    let entry = resolver().entry(svc_name, namespace, group, ttl);
    let snapshot = entry.snapshot(ttl).await;
    if snapshot.instances.is_empty() {
        return Err(match snapshot.last_error {
            Some(detail) => ResolveError::RegistryUnavailable {
                svc: svc_name.to_string(),
                detail,
            },
            None => ResolveError::NoHealthyInstance {
                svc: svc_name.to_string(),
            },
        });
    }
    entry
        .balancer
        .choose(&snapshot.instances)
        .ok_or(ResolveError::NoHealthyInstance {
            svc: svc_name.to_string(),
        })
}

/// 实例的基础地址，可直接拼服务内路径。
pub fn instance_base_url(instance: &ServiceInstance) -> String {
    format!("http://{}:{}", instance.ip, instance.port)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(id: &str, port: u16) -> ServiceInstance {
        ServiceInstance {
            namespace: None,
            group: None,
            svc_name: "demo-svr".into(),
            instance_id: id.into(),
            ip: "10.0.0.1".into(),
            port,
            metadata: HashMap::new(),
            health_check_url: None,
        }
    }

    fn test_entry() -> Arc<ResolverEntry> {
        Arc::new(ResolverEntry {
            svc_name: "demo-svr".into(),
            namespace: None,
            group: None,
            discovery: Arc::new(ServiceDiscovery::new("demo-svr", Duration::from_secs(30))),
            balancer: RoundRobinBalancer::new(),
            last_attempt: Mutex::new(None),
            refresh_lock: AsyncMutex::new(()),
        })
    }

    #[test]
    fn failed_attempt_has_shorter_ttl() {
        let ok = Attempt {
            at: Instant::now(),
            error: None,
        };
        let failed = Attempt {
            at: Instant::now(),
            error: Some("consul down".into()),
        };
        assert_eq!(
            ok.effective_ttl(Duration::from_secs(30)),
            Duration::from_secs(30)
        );
        assert_eq!(
            failed.effective_ttl(Duration::from_secs(30)),
            Duration::from_secs(1)
        );
        assert_eq!(
            failed.effective_ttl(Duration::from_millis(200)),
            Duration::from_millis(200)
        );
    }

    #[tokio::test]
    async fn round_robin_is_per_entry() {
        let entry = test_entry();
        let fetch = || async { Ok(vec![instance("a", 9001), instance("b", 9002)]) };
        for expected in [9001, 9002, 9001, 9002] {
            let snap = entry.snapshot_with(Duration::from_secs(30), fetch).await;
            let picked = entry.balancer.choose(&snap.instances).unwrap();
            assert_eq!(picked.port, expected);
        }
    }

    #[tokio::test]
    async fn empty_result_is_cached_within_ttl() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let entry = test_entry();
        let calls = Arc::new(AtomicUsize::new(0));
        let ttl = Duration::from_secs(30);

        for _ in 0..5 {
            let counter = Arc::clone(&calls);
            let snap = entry
                .snapshot_with(ttl, || async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    Ok(Vec::new())
                })
                .await;
            assert!(snap.instances.is_empty());
            assert!(snap.last_error.is_none());
        }
        // 负缓存生效：只有第一次真正查了注册中心
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn expired_cache_refreshes_and_sees_recovery() {
        let entry = test_entry();
        let ttl = Duration::from_secs(30);
        let first = entry.snapshot_with(ttl, || async { Ok(Vec::new()) }).await;
        assert!(first.instances.is_empty());

        // 模拟 TTL 过期：把上次尝试的时间往前拨
        entry.record(Attempt {
            at: Instant::now() - Duration::from_secs(31),
            error: None,
        });
        let recovered = entry
            .snapshot_with(ttl, || async { Ok(vec![instance("a", 9001)]) })
            .await;
        assert_eq!(recovered.instances.len(), 1);
        assert!(recovered.last_error.is_none());
    }

    #[tokio::test]
    async fn refresh_failure_keeps_old_snapshot_and_is_throttled() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let entry = test_entry();
        let ttl = Duration::from_secs(30);
        let good = entry
            .snapshot_with(ttl, || async { Ok(vec![instance("a", 9001)]) })
            .await;
        assert_eq!(good.instances.len(), 1);

        entry.record(Attempt {
            at: Instant::now() - Duration::from_secs(31),
            error: None,
        });
        let calls = Arc::new(AtomicUsize::new(0));
        for _ in 0..3 {
            let counter = Arc::clone(&calls);
            let snap = entry
                .snapshot_with(ttl, || async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    Err("consul down".to_string())
                })
                .await;
            assert_eq!(snap.instances.len(), 1, "失败不能抹掉好实例");
            assert_eq!(snap.last_error.as_deref(), Some("consul down"));
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1, "失败也要节流");
    }

    #[tokio::test]
    async fn empty_service_name_is_rejected_before_lookup() {
        let err = choose_instance("  ", None, None, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert_eq!(err, ResolveError::InvalidServiceName);
    }
}
