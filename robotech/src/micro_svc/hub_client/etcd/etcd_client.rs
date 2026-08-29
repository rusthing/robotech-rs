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
use crate::micro_svc::{HubClientError, MicroSvcConfig, RegistryCenterClient};
use async_trait::async_trait;
use etcd_client::GetOptions;
use tokio::sync::watch;

pub struct EtcdClient {
    etcd_client: etcd_client::Client,
    config_key: Option<ConfigKey>,
}

impl EtcdClient {
    const CLIENT_NAME: &'static str = "etcd";

    pub async fn new(micro_svc_config: MicroSvcConfig) -> Result<Self, HubClientError> {
        let MicroSvcConfig {
            svc_name,
            profile,
            etcd: etcd_config,
            ..
        } = micro_svc_config;
        let etcd_config = etcd_config.unwrap(); // 调用new方法前判断etcd_config必须为Some
        let HubClientConfig {
            base_url,
            namespace,
            group,
        } = etcd_config.hub_client.clone();
        let client =
            etcd_client::Client::connect(base_url, Some(etcd_config.connect_options.into()))
                .await
                .map_err(|e| HubClientError::Connection(e.to_string()))?;
        let config_key = etcd_config
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
            etcd_client: client,
            config_key,
        })
    }
}

#[async_trait]
impl ConfigCenterClient for EtcdClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    fn config_key(&self) -> Result<ConfigKey, ConfigCenterError> {
        self.config_key
            .clone()
            .ok_or(ConfigCenterError::Parse("missing config_key".to_string()))
    }

    async fn fetch(&self) -> Result<ConfigItem, ConfigCenterError> {
        let config_key = self.config_key()?;
        let key = self.key()?;
        let mut client = self.etcd_client.clone();
        let resp = client
            .get(key.to_string(), Some(GetOptions::new().with_prefix()))
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
                .ok_or(ConfigCenterError::UnknownFileFormat(key))?,
            content,
            version: Some(kv.mod_revision().to_string()),
        })
    }

    async fn watch(
        &self,
        config_changed_sender: watch::Sender<()>,
    ) -> Result<(), ConfigCenterError> {
        let key = self.key()?;

        // 1) 先在持有锁的情况下创建 WatchStream，然后立即释放锁，
        //    避免后续长连接流阻塞其它 get/publish。
        let watch_stream = {
            let mut etcd_client = self.etcd_client.clone();
            etcd_client
                .watch(key, None)
                .await
                .map_err(|e| ConfigCenterError::Connection(e.to_string()))?
        };

        // 2) 后台任务：消费 WatchStream，把 etcd 事件翻译成 crate 内部的 ConfigEvent。
        tokio::spawn(async move {
            let mut stream = watch_stream;

            while let Ok(Some(resp)) = stream.message().await {
                // 0.19 的 WatchResponse 有三种形态：
                //   * 创建回执：created() == true，events() 通常为空；
                //   * 取消回执：canceled() == true，流到此结束，应当退出；
                //   * 事件响应：events() 里装着本次变更的 KeyValue 列表。
                if resp.canceled() {
                    tracing::warn!(
                        watch_id = resp.watch_id(),
                        reason = %resp.cancel_reason(),
                        "etcd watch canceled"
                    );
                    break;
                }

                if !resp.events().is_empty() {
                    // 整批事件只发一次通知
                    if config_changed_sender.send(()).is_err() {
                        return;
                    }
                }
            }
        });
        Ok(())
    }
}

#[async_trait]
impl RegistryCenterClient for EtcdClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }
}
