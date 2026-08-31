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
use crate::micro_svc::{HubClientError, MicroSvcConfig, RegistryCenterClient};
use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub struct ConsulClient {
    reqwest_client: reqwest::Client,
    base_url: String,
    blocking_query_timeout: Duration,
    config_key: Option<ConfigKey>,
    config_watch_join_handles: Mutex<Vec<JoinHandle<()>>>,
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

impl Drop for ConsulClient {
    fn drop(&mut self) {
        let handles: Vec<_> = std::mem::take(&mut *self.config_watch_join_handles.lock().unwrap());
        for handle in handles {
            handle.abort();
        }
    }
}

impl ConsulClient {
    const CLIENT_NAME: &'static str = "consul";

    pub fn new(micro_svc_config: MicroSvcConfig) -> Result<Self, HubClientError> {
        let MicroSvcConfig {
            svc_name,
            profile,
            consul: consul_config,
            ..
        } = micro_svc_config;
        let consul_config = consul_config.unwrap(); // 调用new方法前判断consul_config必须为Some
        let HubClientConfig {
            base_url,
            namespace,
            group,
        } = consul_config.hub_client.clone();
        let base_url = base_url[0].trim_end_matches('/').to_string();
        let blocking_query_timeout = consul_config.blocking_query_timeout;
        let config_key = consul_config
            .config
            .clone()
            .map(|config| -> Result<ConfigKey, HubClientError> {
                let group = group.or_else(|| profile.clone());
                let mut data_id =
                    svc_name.ok_or(HubClientError::Config("svc_name is required".to_string()))?;
                data_id = format!("{}.{}", data_id, config.file_format);
                Ok(ConfigKey::new(namespace, group, data_id))
            })
            .transpose()?;
        Ok(Self {
            reqwest_client: reqwest::Client::new(),
            base_url,
            blocking_query_timeout,
            config_key,
            config_watch_join_handles: Mutex::new(Vec::new()),
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

#[async_trait]
impl ConfigCenterClient for ConsulClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    fn config_key(&self) -> Result<ConfigKey, ConfigCenterError> {
        self.config_key
            .clone()
            .ok_or(ConfigCenterError::Parse("missing config_key".to_string()))
    }

    async fn fetch(&self, key: &ConfigKey) -> Result<ConfigItem, ConfigCenterError> {
        let consul_key = key.to_string();
        let entry = Self::fetch_kv_raw(
            &self.reqwest_client,
            &self.base_url,
            &consul_key,
            &None,
            &self.blocking_query_timeout,
        )
        .await
        .map_err(|e| ConfigCenterError::Connection(e.to_string()))?
        .ok_or(ConfigCenterError::NotFound(key.clone()))?;

        let raw = entry
            .value
            .ok_or(ConfigCenterError::NotFound(key.clone()))?;
        let content = decode_value(&raw).map_err(|e| ConfigCenterError::Parse(e.to_string()))?;
        Ok(ConfigItem {
            key: key.clone(),
            format: key
                .infer_file_format()
                .ok_or(ConfigCenterError::UnknownFileFormat(consul_key.to_string()))?,
            content,
            version: Some(entry.modify_index.to_string()),
        })
    }

    async fn watch(
        &self,
        key: &ConfigKey,
        config_changed_sender: watch::Sender<()>,
    ) -> Result<(), ConfigCenterError> {
        let reqwest_client = self.reqwest_client.clone();
        let base_url = self.base_url.to_string();
        let consul_key = key.to_string();
        let blocking_query_timeout = self.blocking_query_timeout;

        let join_handle = tokio::spawn(async move {
            let mut last_index: Option<String> = None;
            loop {
                match Self::fetch_kv_raw(
                    &reqwest_client,
                    &base_url,
                    &consul_key,
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

        self.config_watch_join_handles.lock().unwrap().push(join_handle);

        Ok(())
    }
}

#[async_trait]
impl RegistryCenterClient for ConsulClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }
}