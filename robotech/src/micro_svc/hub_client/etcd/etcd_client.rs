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
    /// 当前活跃的租约状态。
    /// * register() 每次都会生成**新的** lease_id 替换旧的
    /// * deregister() / drop 时会 revoke 掉这个 lease，key 会立刻从 etcd 删除
    /// * 进程 crash / etcd 网络不可达超过 LEASE_TTL_SECS 后，etcd 服务端会自动 expire 该 lease 关联的所有 key
    lease_state: Mutex<Option<LeaseState>>,
}

/// 一次租约生命周期的内部状态。
///
/// 为什么既要记 `lease_id` 又要记 `keep_alive_handle`：
/// - deregister / 重新 register 时需要主动 revoke lease → 必须拿到 lease_id
/// - 主动清理时需要先 abort 续租后台 task，避免它和 revoke 并发继续续租
struct LeaseState {
    lease_id: i64,
    keep_alive_handle: JoinHandle<()>,
}

impl EtcdClient {
    const CLIENT_NAME: &'static str = "etcd";
    /// 租约 TTL（秒）。续租失败超过这个时间 etcd 会自动删除绑定 key，保证不会有脏实例。
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

#[cfg(feature = "registry-center")]
#[async_trait]
impl RegistryCenterClient for EtcdClient {
    fn name(&self) -> &'static str {
        Self::CLIENT_NAME
    }

    /// 将服务实例注册到 etcd。
    ///
    /// 流程（Etcd 最大坑在这里，**绝对不能写永久 key**）：
    /// 1. `lease_grant(30s)` 申请一个带 TTL 的租约
    /// 2. `put(key, value, PutOptions.with_lease(lease_id))` 把实例 key 绑定到这个租约上
    /// 3. `lease_keep_alive` 拿到续租句柄，spawn 后台 task 每 TTL/3（≈10s）续租一次
    /// 4. 如果 register 被重复调用（比如 keeper 30s 兜底刷新）：先停掉旧续租 task + revoke 旧 lease，旧 key 立即过期删除
    ///
    /// 这样进程 crash / 断网超过 30s，etcd 服务端会**自动删除**绑定 key，不会有脏实例一直留在表里。
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

        // Step 1: 申请 30s TTL 租约
        let lease_resp = client
            .lease_grant(Self::LEASE_TTL_SECS, None)
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
        let lease_id = lease_resp.id();

        // Step 2: put 绑定 lease_id，注意这里**没有 None**，绑定了 lease 才能自动过期
        let put_opts = PutOptions::new().with_lease(lease_id);
        client
            .put(key, value, Some(put_opts))
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;

        // Step 3: 开 keep_alive 通道，spawn 后台续租 task
        let (mut keeper, _stream) = client
            .lease_keep_alive(lease_id)
            .await
            .map_err(|e| RegistryCenterError::Connection(e.to_string()))?;

        // 每 TTL/3 续租一次，留 2/3 的余量应对网络抖动。
        // 失败直接 break：etcd 会在 TTL 后自动删 key，外部 keeper 30s 兜底时又会重新 grant+put。
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

        // Step 4: 记录新 lease_state，同时清理旧的。
        // 典型场景：instance_id 变了（端口更换）→ reregister → 新旧 lease 同时存在。
        // 必须在这里主动 revoke 旧 lease，旧 key 才会秒没。
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

    /// 从 etcd 注销服务实例。
    ///
    /// 1. 先主动 revoke lease：key 立即从 etcd 删除（比等 TTL 快），同时停掉续租 task
    /// 2. 再扫一遍 registry 前缀按 instance_id delete 兜底（防止旧 key 没绑 lease 的历史遗留脏数据）
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
