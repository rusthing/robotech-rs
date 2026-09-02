//! Consul 后端适配器。
//!
//! Consul 的 KV 是语言无关的标准 HTTP API，这里直接拼 `/v1/kv/...` 调用，不依赖
//! 任何 Consul 专用 SDK。变更感知用 Consul 的 blocking query 机制：带着上一次拿到的
//! `ModifyIndex` 去发起下一次请求，Consul agent 会一直 hold 住连接，直到该 key 有
//! 变化或超时才返回——本质是长轮询，不是真正的服务端推送，所以watch的实时性弱于
//! etcd/Nacos，但胜在协议极其简单、不需要额外依赖。

use crate::micro_svc::config_center::{
    ConfigCenterClient, ConfigCenterError, ConfigItem, ConfigKey,
};
use crate::micro_svc::hub_client_config::HubClientConfig;
use crate::micro_svc::{
    HubClientError, MicroSvcConfig, RegistryCenterClient, RegistryCenterError, ServiceInstance,
};
use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub struct ConsulClient {
    reqwest_client: reqwest::Client,
    base_url: String,
    blocking_query_timeout: Duration,
}

#[derive(Deserialize)]
struct ConsulKvEntry {
    /// Consul 返回的值(base64 编码，需要解码后才能使用)
    #[serde(rename = "Value")]
    value: Option<String>,
    /// Consul 返回的修改索引，用于 next request 的 blocking query
    #[serde(rename = "ModifyIndex")]
    modify_index: u64,
}

impl ConsulClient {
    const CLIENT_NAME: &'static str = "consul";

    pub fn new(micro_svc_config: MicroSvcConfig) -> Result<Self, HubClientError> {
        let MicroSvcConfig {
            consul: consul_config,
            ..
        } = micro_svc_config;
        let consul_config = consul_config.unwrap(); // 调用new方法前判断consul_config必须为Some
        let HubClientConfig { base_url, .. } = consul_config.hub_client.clone();
        let base_url = base_url[0].trim_end_matches('/').to_string();
        let blocking_query_timeout = consul_config.blocking_query_timeout;
        Ok(Self {
            reqwest_client: reqwest::Client::new(),
            base_url,
            blocking_query_timeout,
        })
    }

    /// 获取 Consul 中指定 key 的值
    /// 正常查询（不带 index 参数），Consul 会立即返回当前值 + 一个 X-Consul-Index 响应头（也叫 ModifyIndex）
    /// 阻塞查询（带 index + wait）, Consul 收到这个请求后会：
    /// 1. 对比当前 key 的 ModifyIndex 是否已经大于 index 参数的值
    /// 2. 如果已变化 → 立即返回新值
    /// 3. 如果没变化 → hold 住 HTTP 连接，最长等待 wait 参数的值（即 blocking_query_timeout）
    /// 4. 期间 key 一旦变化就立即返回
    /// 5. 如果超时了还没变 → 返回和之前相同的响应
    async fn fetch_kv_raw(
        reqwest_client: &reqwest::Client,
        base_url: &str,
        key: &str,
        last_index: &Option<String>,
        blocking_query_timeout: &Duration,
    ) -> Result<Option<ConsulKvEntry>, ConfigCenterError> {
        let mut url = format!("{}/v1/kv/{}", base_url, key);
        if let Some(idx) = last_index {
            url = format!(
                "{url}?index={idx}&wait={}s",
                blocking_query_timeout.as_secs()
            );
        }
        let resp = reqwest_client
            .get(&url)
            .send()
            .await
            .map_err(|e| ConfigCenterError::Connection(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let entries: Vec<ConsulKvEntry> = resp
            .json()
            .await
            .map_err(|e| ConfigCenterError::Parse(e.to_string()))?;
        Ok(entries.into_iter().next())
    }
}

/// Consul 返回的值是 base64 编码，需要解码后才能使用
fn decode_value(raw: &str) -> Result<String, ConfigCenterError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|e| ConfigCenterError::Parse(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| ConfigCenterError::Parse(e.to_string()))
}

fn build_svc_name(service_instance: ServiceInstance) -> String {
    let namespace = if let Some(namespace) = &service_instance.namespace {
        format!("{}-", namespace)
    } else {
        "".to_string()
    };
    let group = if let Some(group) = &service_instance.group {
        format!("{}-", group)
    } else {
        "".to_string()
    };
    format!("{}{}{}", namespace, group, service_instance.svc_name)
}

#[async_trait]
impl ConfigCenterClient for ConsulClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    async fn fetch(&self, config_key: &ConfigKey) -> Result<ConfigItem, ConfigCenterError> {
        let consul_key = config_key.to_string();
        let entry = Self::fetch_kv_raw(
            &self.reqwest_client,
            &self.base_url,
            &consul_key,
            &None,
            &self.blocking_query_timeout,
        )
        .await
        .map_err(|e| ConfigCenterError::Connection(e.to_string()))?
        .ok_or(ConfigCenterError::NotFound(config_key.clone()))?;

        let raw = entry
            .value
            .ok_or(ConfigCenterError::NotFound(config_key.clone()))?;
        let content = decode_value(&raw).map_err(|e| ConfigCenterError::Parse(e.to_string()))?;
        Ok(ConfigItem {
            key: config_key.clone(),
            format: config_key
                .infer_file_format()
                .ok_or(ConfigCenterError::UnknownFileFormat(consul_key.to_string()))?,
            content,
        })
    }

    async fn watch(
        &self,
        config_key: &ConfigKey,
        config_changed_sender: watch::Sender<()>,
    ) -> Result<Option<JoinHandle<()>>, ConfigCenterError> {
        let reqwest_client = self.reqwest_client.clone();
        let base_url = self.base_url.to_string();
        let config_key = config_key.to_string();
        let blocking_query_timeout = self.blocking_query_timeout;

        let join_handle = tokio::spawn(async move {
            let mut last_index: Option<String> = None;
            loop {
                match Self::fetch_kv_raw(
                    &reqwest_client,
                    &base_url,
                    &config_key,
                    &last_index,
                    &blocking_query_timeout,
                )
                .await
                {
                    Ok(Some(entry)) => {
                        if let Some(ref last_idx) = last_index {
                            if entry.modify_index.to_string() == *last_idx {
                                continue;
                            }
                        }
                        last_index = Some(entry.modify_index.to_string());
                        if config_changed_sender.send(()).is_err() {
                            return;
                        }
                    }
                    Ok(None) => {
                        if config_changed_sender.send(()).is_err() {
                            return;
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                    Err(_) => {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                }
            }
        });

        Ok(Some(join_handle))
    }
}

#[async_trait]
impl RegistryCenterClient for ConsulClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    async fn register(
        &self,
        service_instance: &ServiceInstance,
    ) -> Result<(), RegistryCenterError> {
        let url = format!("{}/v1/agent/service/register", self.base_url);
        let svc_name = build_svc_name(service_instance.clone());
        let body = serde_json::json!({
            "ID": service_instance.instance_id,
            "Name": svc_name,
            "Address": service_instance.ip,
            "Port": service_instance.port,
            "Meta": service_instance.metadata,
            "Check": service_instance.health_check_url.as_ref().map(|url| {
                serde_json::json!({
                    "HTTP": url,
                    "Interval": "10s",
                    "DeregisterCriticalServiceAfter": "30s"
                })
            }),
        });
        self.reqwest_client
            .put(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
        Ok(())
    }

    async fn deregister(
        &self,
        service_instance: &ServiceInstance,
    ) -> Result<(), RegistryCenterError> {
        let url = format!(
            "{}/v1/agent/service/deregister/{}",
            self.base_url, service_instance.instance_id
        );
        self.reqwest_client
            .put(&url)
            .send()
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
        Ok(())
    }

    async fn discover(
        &self,
        namespace: Option<String>,
        group: Option<String>,
        svc_name: &str,
    ) -> Result<Vec<ServiceInstance>, RegistryCenterError> {
        #[derive(Deserialize)]
        struct ConsulHealthEntry {
            #[serde(rename = "Service")]
            service: ConsulServiceEntry,
        }

        #[derive(Deserialize)]
        struct ConsulServiceEntry {
            #[serde(rename = "ID")]
            id: String,
            #[serde(rename = "Service")]
            service: String,
            #[serde(rename = "Address")]
            address: String,
            #[serde(rename = "Port")]
            port: u16,
            #[serde(default, rename = "Meta")]
            meta: HashMap<String, String>,
        }

        let url = format!(
            "{}/v1/health/service/{}?passing=true",
            self.base_url, svc_name
        );
        let resp = self
            .reqwest_client
            .get(&url)
            .send()
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
        let entries: Vec<ConsulHealthEntry> = resp
            .json()
            .await
            .map_err(|e| RegistryCenterError::Parse(e.to_string()))?;
        Ok(entries
            .into_iter()
            .map(|entry| ServiceInstance {
                namespace: namespace.clone(),
                group: group.clone(),
                instance_id: entry.service.id,
                svc_name: entry.service.service,
                ip: entry.service.address,
                port: entry.service.port,
                metadata: entry.service.meta,
                health_check_url: None,
            })
            .collect())
    }
}
