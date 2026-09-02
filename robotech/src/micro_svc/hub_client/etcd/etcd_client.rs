//! etcd 后端适配器。
//!
//! etcd 本身只是 KV 存储，没有"配置中心"这个原生概念，所以这里自己定义了一套
//! key 的目录规范：`/{namespace}/config/{group}/{data_id}`，value 就是配置原文。
//! 这套规范只在本 crate 内部使用（对应设计讨论里说的"Layer B"），跟 Nacos/Consul
//! 的线上格式没有任何关系，也不需要有关系。
//!
//! 注意：本文件基于 `etcd-hub_client` crate 已公开文档的 API 编写（`Client::connect` /
//! `hub_client.get` / `hub_client.put` / `hub_client.watch`），但本地沙箱没有网络无法执行
//! `cargo build` 做真实编译校验，接入前建议先 `cargo check --features etcd` 跑一遍，
//! 关注 `kv.value_str()` / `event.event_type()` / `kv.mod_revision()` 这几个方法名
//! 是否与你锁定的具体版本一致。

use crate::micro_svc::config_center::{
    ConfigCenterClient, ConfigCenterError, ConfigItem, ConfigKey,
};
use crate::micro_svc::hub_client_config::HubClientConfig;
use crate::micro_svc::{
    HubClientError, MicroSvcConfig, RegistryCenterClient, RegistryCenterError, ServiceInstance,
};
use async_trait::async_trait;
use etcd_client::{GetOptions, PutOptions};
use std::sync::Mutex;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{error, warn};

pub struct EtcdClient {
    etcd_client: etcd_client::Client,
    lease_state: Mutex<Option<LeaseState>>,
}

struct LeaseState {
    lease_id: i64,
    keep_alive_handle: JoinHandle<()>,
}

impl EtcdClient {
    const CLIENT_NAME: &'static str = "etcd";
    const LEASE_TTL_SECS: i64 = 30;

    pub async fn new(micro_svc_config: MicroSvcConfig) -> Result<Self, HubClientError> {
        let MicroSvcConfig {
            etcd: etcd_config, ..
        } = micro_svc_config;
        let etcd_config = etcd_config.unwrap();
        let HubClientConfig { base_url, .. } = etcd_config.hub_client.clone();
        let client =
            etcd_client::Client::connect(base_url, Some(etcd_config.connect_options.into()))
                .await
                .map_err(|e| HubClientError::Connection(e.to_string()))?;
        Ok(Self {
            etcd_client: client,
            lease_state: Mutex::new(None),
        })
    }
}

#[async_trait]
impl ConfigCenterClient for EtcdClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    async fn fetch(&self, config_key: &ConfigKey) -> Result<ConfigItem, ConfigCenterError> {
        let etcd_config_key = format!("/config/{config_key}");
        let mut client = self.etcd_client.clone();
        let resp = client
            .get(
                etcd_config_key.clone(),
                Some(GetOptions::new().with_prefix()),
            )
            .await
            .map_err(|e| ConfigCenterError::Connection(e.to_string()))?;
        let kv = resp
            .kvs()
            .first()
            .ok_or_else(|| ConfigCenterError::NotFound(config_key.clone()))?;
        let content = kv
            .value_str()
            .map_err(|e| ConfigCenterError::Parse(e.to_string()))?
            .to_string();
        Ok(ConfigItem {
            key: config_key.clone(),
            format: config_key
                .infer_file_format()
                .ok_or(ConfigCenterError::UnknownFileFormat(etcd_config_key))?,
            content,
        })
    }

    async fn watch(
        &self,
        config_key: &ConfigKey,
        config_changed_sender: watch::Sender<()>,
    ) -> Result<Option<JoinHandle<()>>, ConfigCenterError> {
        let etcd_config_key = format!("/config/{config_key}");

        let watch_stream = {
            let mut etcd_client = self.etcd_client.clone();
            etcd_client
                .watch(etcd_config_key, None)
                .await
                .map_err(|e| ConfigCenterError::Connection(e.to_string()))?
        };

        let sender = config_changed_sender.clone();
        let join_handle = tokio::spawn(async move {
            let mut stream = watch_stream;

            while let Ok(Some(resp)) = stream.message().await {
                if resp.canceled() {
                    warn!(
                        watch_id = resp.watch_id(),
                        reason = %resp.cancel_reason(),
                        "etcd watch canceled"
                    );
                    break;
                }

                if !resp.events().is_empty() {
                    if sender.send(()).is_err() {
                        return;
                    }
                }
            }
        });

        Ok(Some(join_handle))
    }
}

#[async_trait]
impl RegistryCenterClient for EtcdClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    async fn register(
        &self,
        service_instance: &ServiceInstance,
    ) -> Result<(), RegistryCenterError> {
        let namespace = if let Some(namespace) = &service_instance.namespace {
            format!("{}/", namespace)
        } else {
            "".to_string()
        };
        let group = if let Some(group) = &service_instance.group {
            format!("{}/", group)
        } else {
            "".to_string()
        };
        let key = format!(
            "/registry/{}{}{}/{}",
            namespace, group, service_instance.svc_name, service_instance.instance_id
        );
        let value = serde_json::to_string(service_instance)
            .map_err(|e| RegistryCenterError::Parse(e.to_string()))?;
        let mut client = self.etcd_client.clone();

        let lease_resp = client
            .lease_grant(Self::LEASE_TTL_SECS, None)
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
        let lease_id = lease_resp.id();

        let put_opts = PutOptions::new().with_lease(lease_id);
        client
            .put(key, value, Some(put_opts))
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;

        let (mut keeper, _stream) = client
            .lease_keep_alive(lease_id)
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;

        let keep_alive_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
                (Self::LEASE_TTL_SECS as u64) / 3,
            ));
            loop {
                interval.tick().await;
                if let Err(e) = keeper.keep_alive().await {
                    error!("etcd lease keep_alive failed (lease_id={lease_id}): {e:?}");
                    break;
                }
            }
        });

        let new_state = LeaseState {
            lease_id,
            keep_alive_handle,
        };
        let old_state = self.lease_state.lock().unwrap().replace(new_state);
        if let Some(old) = old_state {
            old.keep_alive_handle.abort();
            let mut c = self.etcd_client.clone();
            let _ = c.lease_revoke(old.lease_id).await;
        }
        Ok(())
    }

    async fn deregister(
        &self,
        service_instance: &ServiceInstance,
    ) -> Result<(), RegistryCenterError> {
        let prefix = "/registry/".to_string();
        let mut client = self.etcd_client.clone();

        let old_state = self.lease_state.lock().unwrap().take();
        if let Some(state) = old_state {
            state.keep_alive_handle.abort();
            let _ = client.lease_revoke(state.lease_id).await;
        }

        let resp = client
            .get(prefix, Some(GetOptions::new().with_prefix()))
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
        for kv in resp.kvs() {
            let key = kv
                .key_str()
                .map_err(|e| RegistryCenterError::Parse(e.to_string()))?;
            if key.ends_with(&service_instance.instance_id) {
                client
                    .delete(key, None)
                    .await
                    .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn discover(
        &self,
        namespace: Option<String>,
        group: Option<String>,
        svc_name: &str,
    ) -> Result<Vec<ServiceInstance>, RegistryCenterError> {
        let prefix = format!("/registry/{}", svc_name);
        let mut client = self.etcd_client.clone();
        let resp = client
            .get(prefix, Some(GetOptions::new().with_prefix()))
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
        let mut instances = Vec::new();
        for kv in resp.kvs() {
            let value = kv
                .value_str()
                .map_err(|e| RegistryCenterError::Parse(e.to_string()))?;
            let mut service_instance: ServiceInstance = serde_json::from_str(value)
                .map_err(|e| RegistryCenterError::Parse(e.to_string()))?;
            service_instance.namespace = namespace.clone();
            service_instance.group = group.clone();
            instances.push(service_instance);
        }
        Ok(instances)
    }
}
