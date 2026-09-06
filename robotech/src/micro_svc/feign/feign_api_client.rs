use crate::api_client::ApiAuthStrategy;
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

#[derive(Debug, Error)]
pub enum FeignError {
    #[error("no available service instance")]
    NoAvailableInstance,
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
    Feign {
        service_discovery: Arc<ServiceDiscovery>,
        load_balancer: Box<dyn LoadBalancer>,
        failure_tracker: Arc<Mutex<FailureTracker>>,
        max_failures: usize,
        cooldown_duration: Duration,
    },
    Static {
        base_url: String,
        auth: Option<ApiAuthStrategy>,
    },
}

pub struct FeignApiClient {
    mode: FeignMode,
}

impl FeignApiClient {
    pub async fn new_feign(svc_name: &str) -> Self {
        let service_discovery = Arc::new(ServiceDiscovery::new(svc_name, Duration::from_secs(30)));
        if let Err(e) = service_discovery.init().await {
            warn!(
                "service discovery init failed for '{}': {:?}, will retry on first request",
                svc_name, e
            );
        }
        let sd_clone = Arc::clone(&service_discovery);
        sd_clone.start_refresh_loop();
        Self {
            mode: FeignMode::Feign {
                service_discovery,
                load_balancer: Box::new(RoundRobinBalancer::new()),
                failure_tracker: Arc::new(Mutex::new(FailureTracker::new())),
                max_failures: 3,
                cooldown_duration: Duration::from_secs(30),
            },
        }
    }

    pub fn new_static(base_url: String, auth: Option<ApiAuthStrategy>) -> Self {
        Self {
            mode: FeignMode::Static { base_url, auth },
        }
    }

    pub fn with_load_balancer(mut self, load_balancer: impl LoadBalancer + 'static) -> Self {
        if let FeignMode::Feign {
            load_balancer: ref mut lb,
            ..
        } = self.mode
        {
            *lb = Box::new(load_balancer);
        }
        self
    }

    pub fn with_max_failures(mut self, max_failures: usize) -> Self {
        if let FeignMode::Feign {
            max_failures: ref mut mf,
            ..
        } = self.mode
        {
            *mf = max_failures;
        }
        self
    }

    pub fn with_cooldown_duration(mut self, cooldown_duration: Duration) -> Self {
        if let FeignMode::Feign {
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

    pub fn service_discovery(&self) -> Option<&Arc<ServiceDiscovery>> {
        match &self.mode {
            FeignMode::Feign {
                service_discovery, ..
            } => Some(service_discovery),
            FeignMode::Static { .. } => None,
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
            FeignMode::Static { base_url, auth } => {
                ApiClientUtils::request(method, base_url, uri, params, body, headers, auth.as_ref())
                    .await
            }
            FeignMode::Feign {
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

    pub async fn get<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        self.do_request(Method::GET, uri, params, None::<&D>, headers)
            .await
    }

    pub async fn get_bytes<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Vec<u8>, ApiClientError> {
        match &self.mode {
            FeignMode::Static { base_url, auth } => {
                ApiClientUtils::get_bytes(base_url, uri, params, headers, auth.as_ref()).await
            }
            FeignMode::Feign {
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

    pub async fn post<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        self.do_request(Method::POST, uri, None::<&D>, body, headers)
            .await
    }

    pub async fn put<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        headers: Option<&HeaderMap>,
        body: &D,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        self.do_request(Method::PUT, uri, None::<&D>, Some(body), headers)
            .await
    }

    pub async fn delete<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        self.do_request(Method::DELETE, uri, None::<&D>, body, headers)
            .await
    }

    pub async fn multipart(
        &self,
        uri: &str,
        form: reqwest::multipart::Form,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        match &self.mode {
            FeignMode::Static { base_url, auth } => {
                ApiClientUtils::multipart(base_url, uri, form, headers, auth.as_ref()).await
            }
            FeignMode::Feign {
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
