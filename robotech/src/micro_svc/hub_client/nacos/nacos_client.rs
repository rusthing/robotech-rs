//! Nacos 后端适配器，基于官方维护的 `nacos-sdk-rust`（crate 名 `nacos-sdk`）。
//!
//! 三个后端里这是唯一有官方 Rust SDK、且能直接和 Spring Cloud Alibaba 用的
//! Java Nacos 客户端在同一个 Nacos 集群里互认的一个——协议、心跳、推送都是
//! Nacos 服务端原生支持的，不需要像 etcd 那样自己发明一套约定。
//!
//! 注意：`get_config` / `publish_config` / `add_listener` 等方法名及
//! `ConfigResponse::content()` / `md5()` 是按 nacos-sdk-rust 官方 README 里
//! `ConfigServiceBuilder::new(ClientProps::new()...).build()` 的用法推出的，
//! 本地沙箱没有网络无法 `cargo build` 做真实编译校验，接入前请对照你锁定的
//! nacos-sdk 版本的 docs.rs 页面核对一遍方法签名，必要时做小幅调整。

use std::sync::{Arc, Mutex};

use crate::micro_svc::hub_client_config::HubClientConfig;

use crate::micro_svc::{ConfigCenterClient, ConfigCenterError, ConfigItem, ConfigKey};
use crate::micro_svc::{HubClientError, MicroSvcConfig};
use crate::micro_svc::{RegistryCenterClient, RegistryCenterError, ServiceInstance};
use async_trait::async_trait;
use nacos_sdk::api::config::{
    ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder,
};
use nacos_sdk::api::naming::{
    NamingService, NamingServiceBuilder, ServiceInstance as NacosInstance,
};
use nacos_sdk::api::props::ClientProps;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// # 桥接器
/// 把 Nacos SDK 的回调式监听，转换成本 crate 统一的 channel 事件流
pub struct Bridge {
    tx: watch::Sender<()>,
    md5: String,
}

impl ConfigChangeListener for Bridge {
    fn notify(&self, config_response: ConfigResponse) {
        if *config_response.md5() != self.md5 {
            if self.tx.send(()).is_err() {
                return;
            }
        }
    }
}

pub struct NacosClient {
    service: ConfigService,
    naming_service: NamingService,
    config_listeners: Mutex<Vec<(String, String, Arc<Bridge>)>>,
}

impl Drop for NacosClient {
    fn drop(&mut self) {
        let listeners: Vec<_> = std::mem::take(&mut *self.config_listeners.lock().unwrap());
        if !listeners.is_empty() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let service = self.service.clone();
                handle.spawn(async move {
                    for (data_id, group, listener) in listeners {
                        let _ = service.remove_listener(data_id, group, listener).await;
                    }
                });
            }
        }
    }
}

impl NacosClient {
    const CLIENT_NAME: &'static str = "nacos";

    pub async fn new(micro_svc_config: MicroSvcConfig) -> Result<Self, HubClientError> {
        let MicroSvcConfig {
            nacos: nacos_config,
            ..
        } = micro_svc_config;
        let nacos_config = nacos_config.unwrap(); // 调用new方法前判断nacos_config必须为Some
        let HubClientConfig {
            base_url,
            namespace,
            ..
        } = nacos_config.hub_client.clone();
        let namespace = if let Some(namespace) = namespace {
            namespace.clone()
        } else {
            "public".to_string()
        };
        let server_addr = base_url[0].trim_end_matches('/').to_string();
        let props = ClientProps::new()
            .server_addr(server_addr.clone())
            .namespace(namespace.clone());
        let service = ConfigServiceBuilder::new(props.clone())
            .build()
            .await
            .map_err(|e| HubClientError::Connection(e.to_string()))?;
        let naming_service = NamingServiceBuilder::new(props)
            .build()
            .await
            .map_err(|e| HubClientError::Connection(e.to_string()))?;
        Ok(Self {
            service,
            naming_service,
            config_listeners: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl ConfigCenterClient for NacosClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    async fn fetch(&self, key: &ConfigKey) -> Result<ConfigItem, ConfigCenterError> {
        let data_id = key.data_id.clone();
        let group = key
            .group
            .clone()
            .unwrap_or_else(|| "DEFAULT_GROUP".to_string());
        let resp = self
            .service
            .get_config(data_id.clone(), group.clone())
            .await
            .map_err(|e| ConfigCenterError::Connection(e.to_string()))?;
        Ok(ConfigItem {
            key: key.clone(),
            format: key
                .infer_file_format()
                .ok_or(ConfigCenterError::UnknownFileFormat(data_id))?,
            content: resp.content().to_string(),
        })
    }

    async fn watch(
        &self,
        config_key: &ConfigKey,
        config_changed_sender: watch::Sender<()>,
    ) -> Result<Option<JoinHandle<()>>, ConfigCenterError> {
        let data_id = config_key.data_id.clone();
        let group = config_key
            .group
            .clone()
            .unwrap_or_else(|| "DEFAULT_GROUP".to_string());

        let bridge = Arc::new(Bridge {
            tx: config_changed_sender.clone(),
            md5: String::new(),
        });

        self.service
            .add_listener(data_id.clone(), group.clone(), bridge.clone())
            .await
            .map_err(|e| ConfigCenterError::Connection(e.to_string()))?;

        self.config_listeners
            .lock()
            .unwrap()
            .push((data_id, group, bridge));

        Ok(None)
    }
}

#[cfg(feature = "registry-center")]
#[async_trait]
impl RegistryCenterClient for NacosClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    async fn register(
        &self,
        service_instance: &ServiceInstance,
    ) -> Result<(), RegistryCenterError> {
        let nacos_instance = NacosInstance {
            instance_id: Some(service_instance.instance_id.clone()),
            ip: service_instance.ip.clone(),
            port: service_instance.port as i32,
            service_name: Some(service_instance.svc_name.clone()),
            metadata: service_instance.metadata.clone(),
            ..Default::default()
        };
        let group = service_instance.group.clone();
        self.naming_service
            .register_instance(service_instance.svc_name.clone(), group, nacos_instance)
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
        Ok(())
    }

    async fn deregister(
        &self,
        service_instance: &ServiceInstance,
    ) -> Result<(), RegistryCenterError> {
        let nacos_instance = NacosInstance {
            instance_id: Some(service_instance.instance_id.clone()),
            ..Default::default()
        };
        self.naming_service
            .deregister_instance(
                service_instance.svc_name.clone(),
                service_instance.group.clone(),
                nacos_instance,
            )
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
        let instances = self
            .naming_service
            .select_instances(svc_name.to_string(), group.clone(), vec![], false, true)
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
        Ok(instances
            .into_iter()
            .map(|service_instance| ServiceInstance {
                namespace: namespace.clone(),
                group: group.clone(),
                instance_id: service_instance.instance_id.unwrap_or_default(),
                svc_name: service_instance.service_name.unwrap_or_default(),
                ip: service_instance.ip,
                port: service_instance.port as u16,
                metadata: service_instance.metadata,
                health_check_url: None,
            })
            .collect())
    }
}
