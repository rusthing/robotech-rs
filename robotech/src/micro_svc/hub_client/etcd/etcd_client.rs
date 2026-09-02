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
use etcd_client::GetOptions;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::warn;

pub struct EtcdClient {
    etcd_client: etcd_client::Client,
}

impl EtcdClient {
    const CLIENT_NAME: &'static str = "etcd";

    pub async fn new(micro_svc_config: MicroSvcConfig) -> Result<Self, HubClientError> {
        let MicroSvcConfig {
            etcd: etcd_config, ..
        } = micro_svc_config;
        let etcd_config = etcd_config.unwrap(); // 调用new方法前判断etcd_config必须为Some
        let HubClientConfig { base_url, .. } = etcd_config.hub_client.clone();
        let client =
            etcd_client::Client::connect(base_url, Some(etcd_config.connect_options.into()))
                .await
                .map_err(|e| HubClientError::Connection(e.to_string()))?;
        Ok(Self {
            etcd_client: client,
        })
    }
}

#[async_trait]
impl ConfigCenterClient for EtcdClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    async fn fetch(&self, key: &ConfigKey) -> Result<ConfigItem, ConfigCenterError> {
        let etcd_key = key.to_string();
        let mut client = self.etcd_client.clone();
        let resp = client
            .get(etcd_key.clone(), Some(GetOptions::new().with_prefix()))
            .await
            .map_err(|e| ConfigCenterError::Connection(e.to_string()))?;
        let kv = resp
            .kvs()
            .first()
            .ok_or_else(|| ConfigCenterError::NotFound(key.clone()))?;
        let content = kv
            .value_str()
            .map_err(|e| ConfigCenterError::Parse(e.to_string()))?
            .to_string();
        Ok(ConfigItem {
            key: key.clone(),
            format: key
                .infer_file_format()
                .ok_or(ConfigCenterError::UnknownFileFormat(etcd_key))?,
            content,
        })
    }

    async fn watch(
        &self,
        config_key: &ConfigKey,
        config_changed_sender: watch::Sender<()>,
    ) -> Result<Option<JoinHandle<()>>, ConfigCenterError> {
        let config_key = config_key.to_string();

        let watch_stream = {
            let mut etcd_client = self.etcd_client.clone();
            etcd_client
                .watch(config_key, None)
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
        client
            .put(key, value, None)
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
        Ok(())
    }

    async fn deregister(
        &self,
        service_instance: &ServiceInstance,
    ) -> Result<(), RegistryCenterError> {
        let prefix = "/registry/".to_string();
        let mut client = self.etcd_client.clone();
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
