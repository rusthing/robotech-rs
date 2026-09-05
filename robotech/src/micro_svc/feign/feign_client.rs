use crate::api_client::ApiClient;
use crate::api_client::ApiClientError;
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
use wheel_rs::addr_utils::Addr;

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

pub struct FeignClient {
    service_discovery: Arc<ServiceDiscovery>,
    load_balancer: Box<dyn LoadBalancer>,
    failure_tracker: Arc<Mutex<FailureTracker>>,
    max_failures: usize,
    cooldown_duration: Duration,
}

impl FeignClient {
    pub fn new(
        service_discovery: Arc<ServiceDiscovery>,
        load_balancer: impl LoadBalancer + 'static,
        max_failures: usize,
        cooldown_duration: Duration,
    ) -> Self {
        Self {
            service_discovery,
            load_balancer: Box::new(load_balancer),
            failure_tracker: Arc::new(Mutex::new(FailureTracker::new())),
            max_failures,
            cooldown_duration,
        }
    }

    pub async fn new_default(svc_name: &str) -> Self {
        let service_discovery = Arc::new(ServiceDiscovery::new(svc_name, Duration::from_secs(30)));
        if let Err(e) = service_discovery.init().await {
            warn!(
                "service discovery init failed for '{}': {:?}, will retry on first request",
                svc_name, e
            );
        }
        let sd_clone = Arc::clone(&service_discovery);
        sd_clone.start_refresh_loop();
        Self::new(
            service_discovery,
            RoundRobinBalancer::new(),
            3,
            Duration::from_secs(30),
        )
    }

    pub fn service_discovery(&self) -> &Arc<ServiceDiscovery> {
        &self.service_discovery
    }

    fn get_available_instances(&self) -> Vec<ServiceInstance> {
        let instances = self.service_discovery.get_instances();
        let tracker = self.failure_tracker.lock().unwrap();
        let now = Instant::now();
        instances
            .iter()
            .filter(|i| !tracker.is_in_cooldown(&i.instance_id, now))
            .cloned()
            .collect()
    }

    fn record_failure(&self, instance_id: &str) {
        let mut tracker = self.failure_tracker.lock().unwrap();
        tracker.record_failure(instance_id, self.max_failures, self.cooldown_duration);
    }

    fn record_success(&self, instance_id: &str) {
        let mut tracker = self.failure_tracker.lock().unwrap();
        tracker.record_success(instance_id);
    }

    async fn do_request<D, E>(
        &self,
        method: &Method,
        uri: &str,
        params: Option<&D>,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        let all_instances = self.service_discovery.get_instances();
        let max_retries = all_instances.len().max(1);

        let mut tried_ids = Vec::new();

        for _ in 0..max_retries {
            let available = self.get_available_instances();
            let candidates: Vec<ServiceInstance> = available
                .into_iter()
                .filter(|i| !tried_ids.contains(&i.instance_id))
                .collect();

            let instance = match self.load_balancer.choose(&candidates) {
                Some(inst) => inst,
                None => break,
            };
            tried_ids.push(instance.instance_id.clone());

            let addr = Addr::new(instance.ip, Some(instance.port));
            let api_client = ApiClient::new_from_addr(addr.clone());

            let result = api_client
                .request(method.clone(), uri, params, body, headers.cloned(), None)
                .await;

            match result {
                Ok(resp) => {
                    self.record_success(&instance.instance_id);
                    return Ok(resp);
                }
                Err(e) => {
                    warn!(
                        "feign call failed for instance {} ({}) on {}: {:?}",
                        instance.instance_id, addr, uri, e
                    );
                    self.record_failure(&instance.instance_id);
                }
            }
        }

        Err(ApiClientError::NotInit(
            FeignError::NoAvailableInstance.to_string(),
        ))
    }

    pub async fn request<D, E>(
        &self,
        method: Method,
        uri: &str,
        params: Option<&D>,
        body: Option<&D>,
        headers: Option<HeaderMap>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        self.do_request(&method, uri, params, body, headers.as_ref())
            .await
    }

    pub async fn get<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        self.do_request(&Method::GET, uri, params, None::<&D>, headers.as_ref())
            .await
    }

    pub async fn get_bytes<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<HeaderMap>,
    ) -> Result<Vec<u8>, ApiClientError> {
        let all_instances = self.service_discovery.get_instances();
        let max_retries = all_instances.len().max(1);
        let mut tried_ids = Vec::new();

        for _ in 0..max_retries {
            let available = self.get_available_instances();
            let candidates: Vec<ServiceInstance> = available
                .into_iter()
                .filter(|i| !tried_ids.contains(&i.instance_id))
                .collect();

            let instance = match self.load_balancer.choose(&candidates) {
                Some(inst) => inst,
                None => break,
            };
            tried_ids.push(instance.instance_id.clone());

            let addr = Addr::new(instance.ip, Some(instance.port));
            let api_client = ApiClient::new_from_addr(addr.clone());

            let result = api_client
                .get_bytes(uri, params, headers.clone(), None)
                .await;

            match result {
                Ok(bytes) => {
                    self.record_success(&instance.instance_id);
                    return Ok(bytes);
                }
                Err(e) => {
                    warn!(
                        "feign get_bytes failed for instance {} ({}): {:?}",
                        instance.instance_id, addr, e
                    );
                    self.record_failure(&instance.instance_id);
                }
            }
        }

        Err(ApiClientError::NotInit(
            FeignError::NoAvailableInstance.to_string(),
        ))
    }

    pub async fn post<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        self.do_request(&Method::POST, uri, None::<&D>, body, headers.as_ref())
            .await
    }

    pub async fn multipart<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        form: reqwest::multipart::Form,
        headers: Option<HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let available = self.get_available_instances();
        let instance = match self.load_balancer.choose(&available) {
            Some(inst) => inst,
            None => {
                return Err(ApiClientError::NotInit(
                    FeignError::NoAvailableInstance.to_string(),
                ));
            }
        };

        let addr = Addr::new(instance.ip, Some(instance.port));
        let api_client = ApiClient::new_from_addr(addr.clone());

        let result = api_client.multipart(uri, form, headers.clone(), None).await;

        match result {
            Ok(resp) => {
                self.record_success(&instance.instance_id);
                Ok(resp)
            }
            Err(e) => {
                warn!(
                    "feign multipart failed for instance {} ({}): {:?}",
                    instance.instance_id, addr, e
                );
                self.record_failure(&instance.instance_id);
                Err(e)
            }
        }
    }
}
