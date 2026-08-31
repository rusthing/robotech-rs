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

use async_trait::async_trait;
use nacos_sdk::api::config::{
    ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder,
};
use nacos_sdk::api::props::ClientProps;
use tokio::sync::watch;

use crate::micro_svc::hub_client_config::HubClientConfig;
use crate::micro_svc::{
    ConfigCenterClient, ConfigCenterError, ConfigItem, ConfigKey, HubClientError, MicroSvcConfig,
    RegistryCenterClient,
};

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
    config_key: Option<ConfigKey>,
    config_listener: Mutex<Vec<(String, String, Arc<Bridge>)>>,
}

impl Drop for NacosClient {
    fn drop(&mut self) {
        let listeners: Vec<_> = std::mem::take(&mut *self.config_listener.lock().unwrap());
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
            svc_name,
            profile,
            nacos: nacos_config,
            ..
        } = micro_svc_config;
        let nacos_config = nacos_config.unwrap(); // 调用new方法前判断nacos_config必须为Some
        let HubClientConfig {
            base_url,
            namespace,
            group,
        } = nacos_config.hub_client.clone();
        let namespace = if let Some(namespace) = namespace {
            namespace.clone()
        } else {
            "public".to_string()
        };
        // 如果 group 和 profile 都为空，使用 DEFAULT_GROUP 作为默认group
        let group = group.or_else(|| Some(profile.unwrap_or("DEFAULT_GROUP".to_string())));
        let server_addr = base_url[0].trim_end_matches('/').to_string();
        let config_key = nacos_config
            .config
            .clone()
            .map(|config| -> Result<ConfigKey, HubClientError> {
                let mut data_id =
                    svc_name.ok_or(HubClientError::Config("svc_name is required".to_string()))?;
                data_id = format!("{}.{}", data_id, config.file_format);
                Ok(ConfigKey::new(Some(namespace.clone()), group, data_id))
            })
            .transpose()?;
        let props = ClientProps::new()
            .server_addr(server_addr)
            .namespace(namespace);
        let service = ConfigServiceBuilder::new(props)
            .build()
            .await
            .map_err(|e| HubClientError::Connection(e.to_string()))?;
        Ok(Self {
            service,
            config_key,
            config_listener: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl ConfigCenterClient for NacosClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    fn config_key(&self) -> Result<ConfigKey, ConfigCenterError> {
        self.config_key
            .clone()
            .ok_or(ConfigCenterError::Parse("missing config_key".to_string()))
    }

    async fn fetch(&self, key: &ConfigKey) -> Result<ConfigItem, ConfigCenterError> {
        let data_id = key.data_id.clone();
        let group = key
            .group
            .clone()
            .ok_or(ConfigCenterError::Parse("missing group".to_string()))?;
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
            version: Some(resp.md5().to_string()),
        })
    }

    async fn watch(
        &self,
        key: &ConfigKey,
        config_changed_sender: watch::Sender<()>,
    ) -> Result<(), ConfigCenterError> {
        let data_id = key.data_id.clone();
        let group = key
            .group
            .clone()
            .ok_or(ConfigCenterError::Parse("missing group".to_string()))?;

        let bridge = Arc::new(Bridge {
            tx: config_changed_sender,
            md5: String::new(),
        });

        self.service
            .add_listener(data_id.clone(), group.clone(), bridge.clone())
            .await
            .map_err(|e| ConfigCenterError::Connection(e.to_string()))?;

        self.config_listener
            .lock()
            .unwrap()
            .push((data_id, group, bridge));

        Ok(())
    }
}

#[async_trait]
impl RegistryCenterClient for NacosClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }
}