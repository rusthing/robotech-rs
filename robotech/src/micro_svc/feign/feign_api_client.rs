use crate::api_client::ApiAuthStrategy;
use crate::api_client::ApiClientConfig;
use crate::api_client::ApiClientError;
use crate::api_client::ApiClientUtils;
use crate::micro_svc::feign::load_balancer::{LoadBalancer, RoundRobinBalancer};
use crate::micro_svc::feign::service_discovery::ServiceDiscovery;
use crate::micro_svc::ServiceInstance;
use crate::api::Ro;
use http::Method;
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

#[derive(Debug, Clone)]
struct FailureRecord {
    /// 连续失败次数（成功一次即清零）
    count: usize,
    /// 冷却结束时刻；`None` 表示不在冷却里
    cooldown_until: Option<Instant>,
    /// 冷却到期后，那个「试探请求」是否已经放出去了
    ///
    /// half-open 的关键：冷却一到就把该实例放回候选池，会让**所有**并发请求
    /// 一起涌向一个刚刚才失败的实例，它还没缓过来就又被压垮，冷却白等。
    /// 所以只放一个探针出去，成功即恢复，失败则重新进冷却（更久）。
    probe_in_flight: bool,
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

    /// # 这个实例现在能不能被选中
    ///
    /// 三种情况：
    ///
    /// - 没记录（从没失败过）→ 可以；
    /// - 冷却中 → 不可以；
    /// - 冷却到期 → 放一个探针（half-open），其余继续挡着。
    fn is_available(&mut self, instance_id: &str, now: Instant) -> bool {
        let Some(record) = self.records.get_mut(instance_id) else {
            return true;
        };

        match record.cooldown_until {
            None => true,
            Some(until) if now < until => false,
            Some(_) => {
                if record.probe_in_flight {
                    false
                } else {
                    record.probe_in_flight = true;
                    true
                }
            }
        }
    }

    /// # 记一次失败，并按次数拉长冷却
    ///
    /// 冷却时长 = `cooldown * 2^(超出阈值的次数)`，封顶 `max_cooldown`：
    ///
    /// - 第一次触顶：1x（默认 1s）——抖一下而已，很快回来；
    /// - 持续失败：2x、4x…… 一路退到 30s。
    ///
    /// 定长 30s 的旧策略在单副本部署下等于「一次抖动 → 半分钟全站静默」，
    /// 而立刻重试又会把已经过载的下游继续压。指数退避是这两者之间的解。
    fn record_failure(
        &mut self,
        instance_id: &str,
        max_failures: usize,
        cooldown: Duration,
        max_cooldown: Duration,
    ) {
        let entry = self
            .records
            .entry(instance_id.to_string())
            .or_insert(FailureRecord {
                count: 0,
                cooldown_until: None,
                probe_in_flight: false,
            });
        entry.count += 1;
        entry.probe_in_flight = false;

        if entry.count >= max_failures {
            let over = entry.count - max_failures;
            // 移位上限 6，避免持续失败时溢出；再大的值都会被 max_cooldown 截住
            let factor = 1u32 << over.min(6);
            let span = (cooldown * factor).min(max_cooldown);
            entry.cooldown_until = Some(Instant::now() + span);
        }
    }

    fn record_success(&mut self, instance_id: &str) {
        self.records.remove(instance_id);
    }

    /// 只读判断：实例是否仍在冷却期（不触发 half-open 探针）
    ///
    /// `is_available` 会在冷却到期时顺手把 `probe_in_flight` 置位，
    /// 那是「准备发请求」时才该有的副作用；拼错误信息时只需要读，不能改状态。
    fn is_cooling(&self, instance_id: &str, now: Instant) -> bool {
        self.records
            .get(instance_id)
            .and_then(|record| record.cooldown_until)
            .is_some_and(|until| now < until)
    }

    /// 给定实例里有多少个还在冷却
    fn cooling_count(&self, instances: &[ServiceInstance], now: Instant) -> usize {
        instances
            .iter()
            .filter(|instance| self.is_cooling(&instance.instance_id, now))
            .count()
    }
}

enum FeignMode {
    MicroSvc {
        service_discovery: Arc<ServiceDiscovery>,
        load_balancer: Box<dyn LoadBalancer>,
        failure_tracker: Arc<Mutex<FailureTracker>>,
        max_failures: usize,
        cooldown_duration: Duration,
        max_cooldown: Duration,
        timeout: Option<Duration>,
    },
    Simple {
        base_url: String,
        auth: Option<ApiAuthStrategy>,
        timeout: Option<Duration>,
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
                namespace,
                group,
                auth: _,
                max_failures,
                cooldown_duration,
                max_cooldown,
                refresh_interval,
                timeout,
            } => {
                let service_discovery = Arc::new(ServiceDiscovery::with_namespace_and_group(
                    &svc_name,
                    refresh_interval,
                    namespace,
                    group,
                ));
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
                        max_cooldown,
                        timeout: Some(timeout),
                    },
                }
            }
            ApiClientConfig::Simple {
                base_url,
                auth,
                timeout,
            } => Self {
                mode: FeignMode::Simple {
                    base_url,
                    auth,
                    timeout: Some(timeout),
                },
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

    /// 取本次请求用的实例快照；缓存为空时立即补一次服务发现
    ///
    /// `init()` 失败只记告警，正常情况下靠 30s 刷新循环恢复。但启动瞬间注册中心
    /// 抖一下、或刷新恰好连续失败时，实例列表会一直是空的，所有请求都白等一个
    /// 刷新周期。真实请求来了就顺手重查一次，代价只发生在「反正也要失败」的路径上。
    async fn instances_for_request(
        service_discovery: &ServiceDiscovery,
    ) -> Arc<Vec<ServiceInstance>> {
        let instances = service_discovery.get_instances();
        if !instances.is_empty() {
            return instances;
        }
        match service_discovery.refresh().await {
            Ok(fresh) if !fresh.is_empty() => {
                info!(
                    "service discovery: {} 请求时补发现到 {} 个实例",
                    service_discovery.svc_name(),
                    fresh.len()
                );
                fresh
            }
            Ok(_) => instances,
            Err(e) => {
                warn!(
                    "service discovery: {} 请求时补发现失败: {:?}",
                    service_discovery.svc_name(),
                    e
                );
                instances
            }
        }
    }

    /// 这个错误要不要换实例重试、并计入实例熔断
    ///
    /// HTTP 4xx 是**对方正常处理了请求**（登录失效、口令错、参数不合法……），
    /// 实例并没有坏。旧实现把它和连接失败一起计数，连打几次就把整组实例打进
    /// 冷却，下一次即便请求完全合法也直接「无可用实例」——生产上表现为
    /// 「认证服务不可用」，而 Consul 里实例其实全是 passing。
    ///
    /// 5xx / 超时 / 连接失败保留原行为：换实例重试并按连续失败次数冷却。
    fn is_retryable_instance_error(error: &ApiClientError) -> bool {
        match error {
            ApiClientError::NonSuccessStatus(_, status, _) => {
                // `StatusCode::to_string()` 形如 "401 Unauthorized"，取第一段判断
                let code = status.split_whitespace().next().unwrap_or_default();
                !code.starts_with('4')
            }
            _ => true,
        }
    }

    /// 构造「无可用实例」错误，带齐排查所需的上下文
    ///
    /// 旧实现返回 `ApiClientError::NotInit("no available service instance")`，
    /// 既说成「客户端未初始化」，又不说是哪个服务、发现几个、是不是被熔断了。
    fn no_available_instance(
        service_discovery: &ServiceDiscovery,
        instances: &[ServiceInstance],
        failure_tracker: &Mutex<FailureTracker>,
    ) -> ApiClientError {
        let cooling = failure_tracker
            .lock()
            .unwrap()
            .cooling_count(instances, Instant::now());
        ApiClientError::NoAvailableInstance(format!(
            "svc={}，发现 {} 个实例，冷却中 {} 个（发现 0 个查注册中心；全部冷却查下游失败日志）",
            service_discovery.svc_name(),
            instances.len(),
            cooling
        ))
    }

    fn get_available_instances(
        instances: &[ServiceInstance],
        failure_tracker: &Mutex<FailureTracker>,
    ) -> Vec<ServiceInstance> {
        // 需要 `&mut`：冷却到期的实例会被标记「探针已放出」，这是状态变更
        let mut tracker = failure_tracker.lock().unwrap();
        let now = Instant::now();
        instances
            .iter()
            .filter(|i| tracker.is_available(&i.instance_id, now))
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
        max_cooldown: Duration,
    ) {
        let mut tracker = failure_tracker.lock().unwrap();
        tracker.record_failure(instance_id, max_failures, cooldown_duration, max_cooldown);
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
            FeignMode::Simple {
                base_url,
                auth,
                timeout,
            } => {
                ApiClientUtils::request(
                    method,
                    base_url,
                    uri,
                    params,
                    body,
                    headers,
                    auth.as_ref(),
                    *timeout,
                )
                .await
            }
            FeignMode::MicroSvc {
                service_discovery,
                load_balancer,
                failure_tracker,
                max_failures,
                cooldown_duration,
                max_cooldown,
                timeout,
            } => {
                let instances = Self::instances_for_request(service_discovery).await;
                let max_retries = instances.len().max(1);
                let mut tried_ids = Vec::new();
                // 最后一个真实错误：全部实例都试过之后要把它抛出去。
                //
                // 只抛 NotInit(NoAvailableInstance) 会把「对方其实回了 500」这种
                // 明确的服务端错误伪装成「没有可用实例」——单实例部署时尤其误导，
                // 看日志只见到一句 no available service instance，
                // 真正的失败原因（响应体里的 Ro）反而丢了。
                let mut last_error: Option<ApiClientError> = None;

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
                        *timeout,
                    )
                    .await;

                    match result {
                        Ok(resp) => {
                            Self::record_success(failure_tracker, &instance_id);
                            return Ok(resp);
                        }
                        Err(e) => {
                            // 4xx 是对方给的明确答复，实例是健康的：原样交给上层，
                            // 既不换实例重试，也不记失败（见 is_retryable_instance_error）
                            if !Self::is_retryable_instance_error(&e) {
                                return Err(e);
                            }
                            warn!(
                                "feign call failed for instance {} ({}) on {}: {:?}",
                                instance_id, base_url, uri, e
                            );
                            Self::record_failure(
                                failure_tracker,
                                &instance_id,
                                *max_failures,
                                *cooldown_duration,
                                *max_cooldown,
                            );
                            last_error = Some(e);
                        }
                    }
                }

                Err(last_error.unwrap_or_else(|| {
                    Self::no_available_instance(service_discovery, &instances, failure_tracker)
                }))
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
    /// 微服务模式下无可用实例时返回 `ApiClientError::NoAvailableInstance`，
    /// 消息里带服务名、发现的实例数与冷却中的实例数。
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
            FeignMode::Simple {
                base_url,
                auth,
                timeout,
            } => {
                ApiClientUtils::get_bytes(
                    base_url,
                    uri,
                    params,
                    headers,
                    auth.as_ref(),
                    *timeout,
                )
                .await
            }
            FeignMode::MicroSvc {
                service_discovery,
                load_balancer,
                failure_tracker,
                max_failures,
                cooldown_duration,
                max_cooldown,
                timeout,
            } => {
                let instances = Self::instances_for_request(service_discovery).await;
                let max_retries = instances.len().max(1);
                let mut tried_ids = Vec::new();
                let mut last_error: Option<ApiClientError> = None;

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
                        ApiClientUtils::get_bytes(&base_url, uri, params, headers, None, *timeout)
                            .await;

                    match result {
                        Ok(bytes) => {
                            Self::record_success(failure_tracker, &instance_id);
                            return Ok(bytes);
                        }
                        Err(e) => {
                            if !Self::is_retryable_instance_error(&e) {
                                return Err(e);
                            }
                            warn!(
                                "feign get_bytes failed for instance {} ({}): {:?}",
                                instance_id, base_url, e
                            );
                            Self::record_failure(
                                failure_tracker,
                                &instance_id,
                                *max_failures,
                                *cooldown_duration,
                                *max_cooldown,
                            );
                            last_error = Some(e);
                        }
                    }
                }

                Err(last_error.unwrap_or_else(|| {
                    Self::no_available_instance(service_discovery, &instances, failure_tracker)
                }))
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
            FeignMode::Simple {
                base_url,
                auth,
                timeout,
            } => {
                ApiClientUtils::multipart(base_url, uri, form, headers, auth.as_ref(), *timeout)
                    .await
            }
            FeignMode::MicroSvc {
                service_discovery,
                load_balancer,
                failure_tracker,
                max_failures,
                cooldown_duration,
                max_cooldown,
                timeout,
            } => {
                let instances = Self::instances_for_request(service_discovery).await;
                let mut tried_ids = Vec::new();
                let (base_url, instance_id) = match Self::try_select_instance(
                    &instances,
                    failure_tracker,
                    load_balancer,
                    &mut tried_ids,
                ) {
                    Some(x) => x,
                    None => {
                        return Err(Self::no_available_instance(
                            service_discovery,
                            &instances,
                            failure_tracker,
                        ));
                    }
                };

                let result =
                    ApiClientUtils::multipart(&base_url, uri, form, headers, None, *timeout).await;

                match result {
                    Ok(resp) => {
                        Self::record_success(failure_tracker, &instance_id);
                        Ok(resp)
                    }
                    Err(e) => {
                        // 4xx 不能算实例故障：multipart 只试一个实例，误记冷却
                        // 会让后续所有请求都选不到这个实例
                        if !Self::is_retryable_instance_error(&e) {
                            return Err(e);
                        }
                        warn!(
                            "feign multipart failed for instance {} ({}): {:?}",
                            instance_id, base_url, e
                        );
                        Self::record_failure(
                            failure_tracker,
                            &instance_id,
                            *max_failures,
                            *cooldown_duration,
                            *max_cooldown,
                        );
                        Err(e)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status_error(status: &str) -> ApiClientError {
        ApiClientError::NonSuccessStatus("/auth/verify".into(), status.into(), "{}".into())
    }

    fn instance(id: &str) -> ServiceInstance {
        ServiceInstance {
            namespace: None,
            group: None,
            svc_name: "adm-svr".into(),
            instance_id: id.into(),
            ip: "10.0.1.14".into(),
            port: 9007,
            metadata: HashMap::new(),
            health_check_url: None,
        }
    }

    #[test]
    fn http_4xx_is_not_an_instance_failure() {
        for status in [
            "400 Bad Request",
            "401 Unauthorized",
            "404 Not Found",
            "409 Conflict",
        ] {
            assert!(
                !FeignApiClient::is_retryable_instance_error(&status_error(status)),
                "4xx 不该触发换实例重试与冷却: {status}"
            );
        }
        for status in ["500 Internal Server Error", "503 Service Unavailable"] {
            assert!(
                FeignApiClient::is_retryable_instance_error(&status_error(status)),
                "5xx 仍应换实例重试: {status}"
            );
        }
        assert!(FeignApiClient::is_retryable_instance_error(
            &ApiClientError::NotInit("network".into())
        ));
    }

    /// 回归：线上 adm 三个实例因为连续的 401 被全部冷却，
    /// 之后合法请求也变成「no available service instance」。
    #[test]
    fn repeated_auth_failures_do_not_freeze_all_instances() {
        let instances = vec![instance("a"), instance("b"), instance("c")];
        let mut tracker = FailureTracker::new();
        for id in ["a", "b", "c"] {
            for _ in 0..3 {
                let error = status_error("401 Unauthorized");
                if !FeignApiClient::is_retryable_instance_error(&error) {
                    continue;
                }
                tracker.record_failure(id, 3, Duration::from_secs(1), Duration::from_secs(30));
            }
        }
        assert_eq!(tracker.cooling_count(&instances, Instant::now()), 0);
        assert!(tracker.records.is_empty());
    }

    #[test]
    fn no_available_instance_error_carries_diagnostic_context() {
        let discovery = ServiceDiscovery::new("adm-svr", Duration::from_secs(30));
        let instances = vec![instance("a"), instance("b"), instance("c")];
        let mut tracker = FailureTracker::new();
        for id in ["a", "b", "c"] {
            for _ in 0..3 {
                tracker.record_failure(id, 3, Duration::from_secs(1), Duration::from_secs(30));
            }
        }
        let error =
            FeignApiClient::no_available_instance(&discovery, &instances, &Mutex::new(tracker));
        assert!(matches!(
            &error,
            ApiClientError::NoAvailableInstance(_)
        ));
        let msg = error.to_string();
        assert!(msg.contains("adm-svr"), "{msg}");
        assert!(msg.contains("发现 3 个"), "{msg}");
        assert!(msg.contains("冷却中 3 个"), "{msg}");
        assert!(!msg.contains("未初始化"), "{msg}");
    }

    #[test]
    fn cooling_count_is_read_only() {
        let instances = vec![instance("a")];
        let mut tracker = FailureTracker::new();
        for _ in 0..3 {
            tracker.record_failure("a", 3, Duration::from_secs(1), Duration::from_secs(30));
        }
        assert_eq!(tracker.cooling_count(&instances, Instant::now()), 1);

        // 冷却到期后只读判断应为 false，且不会顺手放出 half-open 探针
        let expired = Instant::now()
            .checked_sub(Duration::from_secs(2))
            .unwrap();
        tracker.records.get_mut("a").unwrap().cooldown_until = Some(expired);
        assert_eq!(tracker.cooling_count(&instances, Instant::now()), 0);
        assert!(!tracker.records["a"].probe_in_flight);
    }
}