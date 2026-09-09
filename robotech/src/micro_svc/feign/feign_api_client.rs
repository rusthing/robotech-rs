use crate::api_client::ApiAuthStrategy;
use crate::api_client::ApiClientConfig;
use crate::api_client::ApiClientError;
use crate::api_client::ApiClientUtils;
use crate::micro_svc::feign::load_balancer::{LoadBalancer, RoundRobinBalancer};
use crate::micro_svc::feign::service_discovery::ServiceDiscovery;
use crate::micro_svc::ServiceInstance;
use crate::ro::Ro;
use http::Method;
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::warn;

/// Feign 调用过程中的错误。
#[derive(Debug, Error)]
pub enum FeignError {
    /// 服务发现成功但没有可用实例（实例列表为空或全部处于冷却期）
    #[error("no available service instance")]
    NoAvailableInstance,
    /// 服务发现失败
    #[error("service discovery failed: {0}")]
    Discovery(String),
}

#[derive(Debug, Clone)]
struct FailureRecord {
    count: usize,
    cooldown_until: Option<Instant>,
}

#[derive(Debug)]
struct FailureTracker {
    records: HashMap<String, FailureRecord>,
}

impl FailureTracker {
    fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    fn is_in_cooldown(&self, instance_id: &str, now: Instant) -> bool {
        self.records
            .get(instance_id)
            .and_then(|r| r.cooldown_until)
            .map(|until| now < until)
            .unwrap_or(false)
    }

    fn record_failure(&mut self, instance_id: &str, max_failures: usize, cooldown: Duration) {
        let entry = self
            .records
            .entry(instance_id.to_string())
            .or_insert(FailureRecord {
                count: 0,
                cooldown_until: None,
            });
        entry.count += 1;
        if entry.count >= max_failures {
            entry.cooldown_until = Some(Instant::now() + cooldown);
        }
    }

    fn record_success(&mut self, instance_id: &str) {
        self.records.remove(instance_id);
    }
}

enum FeignMode {
    MicroSvc {
        service_discovery: Arc<ServiceDiscovery>,
        load_balancer: Box<dyn LoadBalancer>,
        failure_tracker: Arc<Mutex<FailureTracker>>,
        max_failures: usize,
        cooldown_duration: Duration,
    },
    Simple {
        base_url: String,
        auth: Option<ApiAuthStrategy>,
    },
}

/// 声明式 HTTP 客户端：根据 `ApiClientConfig` 选择微服务模式或简单模式。
///
/// - 微服务模式：通过 `ServiceDiscovery` 发现目标服务实例，`LoadBalancer` 选择实例，
///   失败实例按 `max_failures` / `cooldown_duration` 进入冷却期，请求会在多个实例间自动重试。
/// - 简单模式：直接请求固定的 `base_url`，可选携带认证策略。
pub struct FeignApiClient {
    mode: FeignMode,
}

impl FeignApiClient {
    /// 根据配置创建 Feign 客户端。
    ///
    /// 微服务模式下会初始化服务发现并启动后台刷新任务；初始化失败不会报错，
    /// 而是记录告警并在首次请求时重试发现。
    pub async fn new(config: ApiClientConfig) -> Self {
        match config {
            ApiClientConfig::MicroSvc {
                svc_name,
                auth: _,
                max_failures,
                cooldown_duration,
                refresh_interval,
            } => {
                let service_discovery =
                    Arc::new(ServiceDiscovery::new(&svc_name, refresh_interval));
                if let Err(e) = service_discovery.init().await {
                    warn!(
                        "service discovery init failed for '{}': {:?}, will retry on first request",
                        svc_name, e
                    );
                }
                let sd_clone = Arc::clone(&service_discovery);
                sd_clone.start_refresh_loop();
                Self {
                    mode: FeignMode::MicroSvc {
                        service_discovery,
                        load_balancer: Box::new(RoundRobinBalancer::new()),
                        failure_tracker: Arc::new(Mutex::new(FailureTracker::new())),
                        max_failures,
                        cooldown_duration,
                    },
                }
            }
            ApiClientConfig::Simple { base_url, auth } => Self {
                mode: FeignMode::Simple { base_url, auth },
            },
        }
    }

    /// 自定义负载均衡器（仅微服务模式生效）。
    ///
    /// 默认使用轮询（RoundRobin）策略，可通过本方法替换。
    pub fn with_load_balancer(mut self, load_balancer: impl LoadBalancer + 'static) -> Self {
        if let FeignMode::MicroSvc {
            load_balancer: ref mut lb,
            ..
        } = self.mode
        {
            *lb = Box::new(load_balancer);
        }
        self
    }

    /// 设置实例进入冷却期所需的连续失败次数（仅微服务模式生效）。
    pub fn with_max_failures(mut self, max_failures: usize) -> Self {
        if let FeignMode::MicroSvc {
            max_failures: ref mut mf,
            ..
        } = self.mode
        {
            *mf = max_failures;
        }
        self
    }

    /// 设置实例进入冷却期后的冷却时长（仅微服务模式生效）。
    pub fn with_cooldown_duration(mut self, cooldown_duration: Duration) -> Self {
        if let FeignMode::MicroSvc {
            cooldown_duration: ref mut cd,
            ..
        } = self.mode
        {
            *cd = cooldown_duration;
        }
        self
    }

    fn get_base_url(host: &str, port: &u16) -> String {
        let protocol = "http";
        format!("{}://{}:{}", protocol, host, port)
    }

    /// 返回内部的服务发现器（仅微服务模式；简单模式返回 `None`）。
    pub fn service_discovery(&self) -> Option<&Arc<ServiceDiscovery>> {
        match &self.mode {
            FeignMode::MicroSvc {
                service_discovery, ..
            } => Some(service_discovery),
            FeignMode::Simple { .. } => None,
        }
    }

    fn get_available_instances(
        instances: &[ServiceInstance],
        failure_tracker: &Mutex<FailureTracker>,
    ) -> Vec<ServiceInstance> {
        let tracker = failure_tracker.lock().unwrap();
        let now = Instant::now();
        instances
            .iter()
            .filter(|i| !tracker.is_in_cooldown(&i.instance_id, now))
            .cloned()
            .collect()
    }

    fn try_select_instance(
        instances: &[ServiceInstance],
        failure_tracker: &Mutex<FailureTracker>,
        load_balancer: &Box<dyn LoadBalancer>,
        tried_ids: &mut Vec<String>,
    ) -> Option<(String, String)> {
        let available = Self::get_available_instances(instances, failure_tracker);
        let candidates: Vec<ServiceInstance> = available
            .into_iter()
            .filter(|i| !tried_ids.contains(&i.instance_id))
            .collect();
        let instance = load_balancer.choose(&candidates)?;
        let instance_id = instance.instance_id.clone();
        let base_url = Self::get_base_url(&instance.ip, &instance.port);
        tried_ids.push(instance_id.clone());
        Some((base_url, instance_id))
    }

    fn record_failure(
        failure_tracker: &Mutex<FailureTracker>,
        instance_id: &str,
        max_failures: usize,
        cooldown_duration: Duration,
    ) {
        let mut tracker = failure_tracker.lock().unwrap();
        tracker.record_failure(instance_id, max_failures, cooldown_duration);
    }

    fn record_success(failure_tracker: &Mutex<FailureTracker>, instance_id: &str) {
        let mut tracker = failure_tracker.lock().unwrap();
        tracker.record_success(instance_id);
    }

    async fn do_request<D, E>(
        &self,
        method: Method,
        uri: &str,
        params: Option<&D>,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        match &self.mode {
            FeignMode::Simple { base_url, auth } => {
                ApiClientUtils::request(method, base_url, uri, params, body, headers, auth.as_ref())
                    .await
            }
            FeignMode::MicroSvc {
                service_discovery,
                load_balancer,
                failure_tracker,
                max_failures,
                cooldown_duration,
            } => {
                let instances = service_discovery.get_instances();
                let max_retries = instances.len().max(1);
                let mut tried_ids = Vec::new();

                for _ in 0..max_retries {
                    let (base_url, instance_id) = match Self::try_select_instance(
                        &instances,
                        failure_tracker,
                        load_balancer,
                        &mut tried_ids,
                    ) {
                        Some(x) => x,
                        None => break,
                    };

                    let result = ApiClientUtils::request(
                        method.clone(),
                        &base_url,
                        uri,
                        params,
                        body,
                        headers,
                        None,
                    )
                    .await;

                    match result {
                        Ok(resp) => {
                            Self::record_success(failure_tracker, &instance_id);
                            return Ok(resp);
                        }
                        Err(e) => {
                            warn!(
                                "feign call failed for instance {} ({}) on {}: {:?}",
                                instance_id, base_url, uri, e
                            );
                            Self::record_failure(
                                failure_tracker,
                                &instance_id,
                                *max_failures,
                                *cooldown_duration,
                            );
                        }
                    }
                }

                Err(ApiClientError::NotInit(
                    FeignError::NoAvailableInstance.to_string(),
                ))
            }
        }
    }

    /// 发起任意方法的 HTTP 请求，返回统一响应结构 `Ro<E>`。
    ///
    /// ## 参数
    /// - `method`：HTTP 方法
    /// - `uri`：请求路径（不含 base_url）
    /// - `params`：查询参数（可选，会被序列化为 query string）
    /// - `body`：请求体（可选）
    /// - `headers`：附加请求头（可选）
    ///
    /// ## 错误
    /// 微服务模式下无可用实例时返回 `ApiClientError::NotInit(FeignError::NoAvailableInstance)`。
    pub async fn request<D, E>(
        &self,
        method: Method,
        uri: &str,
        params: Option<&D>,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        self.do_request(method, uri, params, body, headers).await
    }

    /// 发起 webhook 请求：GET 时 `data` 作为查询参数，其它方法作为请求体。
    pub async fn webhook<D, E>(
        &self,
        method: Method,
        uri: &str,
        data: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        match method {
            Method::GET => self.do_request(method, uri, data, None, headers).await,
            _ => self.do_request(method, uri, None, data, headers).await,
        }
    }

    /// 发起 GET 请求，返回 JSON 值。
    pub async fn get<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        self.do_request(Method::GET, uri, params, None::<&D>, headers)
            .await
    }

    /// 发起 GET 请求并返回原始字节（适用于下载文件等场景）。
    pub async fn get_bytes<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Vec<u8>, ApiClientError> {
        match &self.mode {
            FeignMode::Simple { base_url, auth } => {
                ApiClientUtils::get_bytes(base_url, uri, params, headers, auth.as_ref()).await
            }
            FeignMode::MicroSvc {
                service_discovery,
                load_balancer,
                failure_tracker,
                max_failures,
                cooldown_duration,
            } => {
                let instances = service_discovery.get_instances();
                let max_retries = instances.len().max(1);
                let mut tried_ids = Vec::new();

                for _ in 0..max_retries {
                    let (base_url, instance_id) = match Self::try_select_instance(
                        &instances,
                        failure_tracker,
                        load_balancer,
                        &mut tried_ids,
                    ) {
                        Some(x) => x,
                        None => break,
                    };

                    let result =
                        ApiClientUtils::get_bytes(&base_url, uri, params, headers, None).await;

                    match result {
                        Ok(bytes) => {
                            Self::record_success(failure_tracker, &instance_id);
                            return Ok(bytes);
                        }
                        Err(e) => {
                            warn!(
                                "feign get_bytes failed for instance {} ({}): {:?}",
                                instance_id, base_url, e
                            );
                            Self::record_failure(
                                failure_tracker,
                                &instance_id,
                                *max_failures,
                                *cooldown_duration,
                            );
                        }
                    }
                }

                Err(ApiClientError::NotInit(
                    FeignError::NoAvailableInstance.to_string(),
                ))
            }
        }
    }

    /// 发起 POST 请求，返回 JSON 值。
    pub async fn post<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        self.do_request(Method::POST, uri, None::<&D>, body, headers)
            .await
    }

    /// 发起 PUT 请求，返回 JSON 值。
    pub async fn put<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        headers: Option<&HeaderMap>,
        body: &D,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        self.do_request(Method::PUT, uri, None::<&D>, Some(body), headers)
            .await
    }

    /// 发起 DELETE 请求，返回 JSON 值。
    pub async fn delete<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        self.do_request(Method::DELETE, uri, None::<&D>, body, headers)
            .await
    }

    /// 发起 multipart/form-data 请求，返回 JSON 值。
    pub async fn multipart(
        &self,
        uri: &str,
        form: reqwest::multipart::Form,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        match &self.mode {
            FeignMode::Simple { base_url, auth } => {
                ApiClientUtils::multipart(base_url, uri, form, headers, auth.as_ref()).await
            }
            FeignMode::MicroSvc {
                service_discovery,
                load_balancer,
                failure_tracker,
                max_failures,
                cooldown_duration,
            } => {
                let instances = service_discovery.get_instances();
                let mut tried_ids = Vec::new();
                let (base_url, instance_id) = match Self::try_select_instance(
                    &instances,
                    failure_tracker,
                    load_balancer,
                    &mut tried_ids,
                ) {
                    Some(x) => x,
                    None => {
                        return Err(ApiClientError::NotInit(
                            FeignError::NoAvailableInstance.to_string(),
                        ));
                    }
                };

                let result = ApiClientUtils::multipart(&base_url, uri, form, headers, None).await;

                match result {
                    Ok(resp) => {
                        Self::record_success(failure_tracker, &instance_id);
                        Ok(resp)
                    }
                    Err(e) => {
                        warn!(
                            "feign multipart failed for instance {} ({}): {:?}",
                            instance_id, base_url, e
                        );
                        Self::record_failure(
                            failure_tracker,
                            &instance_id,
                            *max_failures,
                            *cooldown_duration,
                        );
                        Err(e)
                    }
                }
            }
        }
    }
}
