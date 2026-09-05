use wheel_rs::ipnet_utils::get_local_ip;

use crate::cfg::CfgError;
use crate::env::{AppEnv, EnvError, APP_ENV};
use crate::micro_svc::hub_client_config::HubClientConfig;
use crate::micro_svc::{ConfigCenterClient, ConfigCenterConfig, ConfigItem, ConfigKey};
use crate::micro_svc::{ConsulClient, EtcdClient, MicroSvcConfig, NacosClient};
use crate::micro_svc::{
    RegistryCenterClient, RegistryCenterConfig, RegistryCenterError, RegistryKey, ServiceInstance,
};
use crate::web::{get_health_check_uri, get_health_check_url_http_protocol, get_web_listen_port};
use arc_swap::ArcSwapOption;
use config::FileFormat;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

static HUB_CLIENT: ArcSwapOption<HubClient> = ArcSwapOption::const_empty();

static RETRY_NEW_HUB_CLIENT_JOIN_HANDLE: ArcSwapOption<JoinHandle<()>> =
    ArcSwapOption::const_empty();

pub async fn setup_hub_client(micro_svc_config: MicroSvcConfig) {
    info!("setup hub client...: {micro_svc_config:?}");
    // 先注销旧服务
    #[cfg(feature = "registry-center")]
    if let Ok(hub_client) = get_hub_client().as_ref() {
        if let Err(e) = hub_client.deregister().await {
            warn!("deregister failed: {e:?}");
        }
    }

    // 新建hub_client
    match HubClient::new(micro_svc_config.clone()).await {
        Ok(hub_client) => {
            #[cfg(feature = "registry-center")]
            if hub_client.registry_key.is_some() && hub_client.registry.is_none() {
                start_new_hub_client_loop(micro_svc_config);
            }
            HUB_CLIENT.store(Some(Arc::new(hub_client)));
        }
        Err(e) => {
            error!("failed to register hub client: {e:?}");
            start_new_hub_client_loop(micro_svc_config);
        }
    }
}

fn start_new_hub_client_loop(micro_svc_config: MicroSvcConfig) {
    // 停止旧的重试新建客户端的任务
    if let Some(join_handle) = RETRY_NEW_HUB_CLIENT_JOIN_HANDLE.load_full() {
        join_handle.abort();
    }

    let micro_svc_config_clone = micro_svc_config.clone();
    let join_handle = tokio::spawn(async move {
        loop {
            match HubClient::new(micro_svc_config_clone.clone()).await {
                Ok(hub_client) => {
                    if hub_client.registry.is_some() {
                        HUB_CLIENT.store(Some(Arc::new(hub_client)));
                        return;
                    }
                }
                Err(e) => {
                    error!("failed to register hub client: {e:?}");
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
    RETRY_NEW_HUB_CLIENT_JOIN_HANDLE.store(Some(Arc::new(join_handle)));
}

pub async fn drop_hub_client() {
    #[cfg(feature = "registry-center")]
    if let Ok(hub_client) = get_hub_client().as_ref() {
        if let Err(e) = hub_client.deregister().await {
            warn!("deregister failed: {e:?}");
        }
    }
    HUB_CLIENT.store(None);
}

pub fn get_hub_client() -> Result<Arc<HubClient>, CfgError> {
    HUB_CLIENT
        .load_full()
        .ok_or(CfgError::NotInit("HUB_CLIENT not initialized".to_string()))
}

pub async fn get_configs() -> Result<Vec<ConfigItem>, CfgError> {
    let hub_client = get_hub_client()?;
    let config = hub_client.get_configs().await?;
    info!("get config center config: {:?}", config);
    Ok(config)
}

pub async fn discover_service(svc_name: &str) -> Result<Vec<ServiceInstance>, CfgError> {
    let hub_client = get_hub_client()?;
    hub_client
        .discover(svc_name)
        .await
        .map_err(|e| CfgError::NotInit(format!("discover service failed: {e:?}",)))
}

pub async fn watch_config_changed<F, Fut>(on_change: F) -> Result<(), CfgError>
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let hub_client = get_hub_client().map_err(|e| {
        CfgError::NotInit(format!("hub client not initialized: {:?}", e).to_string())
    })?;

    hub_client.watch_config_changed(on_change).await
}

pub async fn register_micro_svc() {
    tokio::spawn(async move {
        loop {
            if let Ok(hub_client) = get_hub_client().as_ref() {
                let retry_interval = hub_client.retry_interval;
                let refresh_interval = hub_client.refresh_interval;
                match hub_client.register().await {
                    Ok(()) => tokio::time::sleep(refresh_interval).await,
                    Err(e) => {
                        warn!("register failed: {e:?}, retry in {retry_interval:?}");
                        tokio::time::sleep(retry_interval).await;
                    }
                }
            } else {
                tokio::time::sleep(Duration::from_secs(5)).await;
            };
        }
    });
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigSnapshot {
    format: String,
    content: String,
}

pub struct HubClient {
    config: Option<Arc<dyn ConfigCenterClient>>,
    config_keys: Option<Vec<ConfigKey>>,
    snapshot_dir: Option<PathBuf>,
    registry: Option<Arc<dyn RegistryCenterClient>>,
    registry_key: Option<RegistryKey>,
    service_instance: OnceLock<ServiceInstance>,
    retry_interval: Duration,
    refresh_interval: Duration,
    join_handles: Mutex<Vec<JoinHandle<()>>>,
}

impl Drop for HubClient {
    fn drop(&mut self) {
        info!("drop hub client...");
        for handle in self.join_handles.get_mut().unwrap().drain(..) {
            handle.abort();
        }
    }
}

impl HubClient {
    pub async fn new(micro_svc_config: MicroSvcConfig) -> Result<Self, CfgError> {
        let (
            config_center_client,
            registry_center_client,
            config_keys,
            snapshot_dir,
            registry_key,
            retry_interval,
            refresh_interval,
        ) = {
            if let Some(consul_config) = micro_svc_config.clone().consul {
                let client = ConsulClient::new(micro_svc_config.clone())
                    .map_err(|e| {
                        warn!(
                            "failed to create consul client: {e:?}, will use snapshot if available",
                        );
                        e
                    })
                    .ok();
                build_branch(
                    &consul_config.hub_client,
                    &micro_svc_config,
                    &consul_config.config,
                    &consul_config.registry,
                    client,
                )?
            } else if let Some(etcd_config) = micro_svc_config.clone().etcd {
                let client = EtcdClient::new(micro_svc_config.clone())
                    .await
                    .map_err(|e| {
                        warn!(
                            "failed to create etcd client: {e:?}, will use snapshot if available",
                        );
                        e
                    })
                    .ok();
                build_branch(
                    &etcd_config.hub_client,
                    &micro_svc_config,
                    &etcd_config.config,
                    &etcd_config.registry,
                    client,
                )?
            } else if let Some(nacos_config) = micro_svc_config.clone().nacos {
                let client = NacosClient::new(micro_svc_config.clone())
                    .await
                    .map_err(|e| {
                        warn!(
                            "failed to create nacos client: {e:?}, will use snapshot if available",
                        );
                        e
                    })
                    .ok();
                build_branch(
                    &nacos_config.hub_client,
                    &micro_svc_config,
                    &nacos_config.config,
                    &nacos_config.registry,
                    client,
                )?
            } else {
                Err(CfgError::NotInit(
                    "no config center client available".to_string(),
                ))?
            }
        };
        Ok(Self {
            config: config_center_client,
            registry: registry_center_client,
            config_keys,
            snapshot_dir,
            registry_key,
            service_instance: OnceLock::new(),
            retry_interval,
            refresh_interval,
            join_handles: Mutex::new(Vec::new()),
        })
    }

    pub async fn get_configs(&self) -> Result<Vec<ConfigItem>, CfgError> {
        let config_center_client = match self.config.as_ref() {
            Some(client) => client,
            None => {
                return Err(CfgError::NotInit(
                    "no config center client available".to_string(),
                ));
            }
        };

        let mut all = Vec::new();
        if let Some(config_keys) = self.config_keys.as_ref() {
            for config_key in config_keys {
                match config_center_client.fetch(config_key).await {
                    Ok(item) => {
                        info!("loaded config: {}", config_key);
                        self.save_snapshot(&item);
                        all.push(item);
                    }
                    Err(e) => {
                        warn!(
                            "failed to fetch config {}: {:?}, trying snapshot",
                            config_key, e
                        );
                        if let Some(item) = self.load_snapshot(config_key) {
                            warn!("using snapshot for config: {}", config_key);
                            all.push(item);
                        } else {
                            return Err(CfgError::Init(e.to_string()));
                        }
                    }
                }
            }
        } else {
            Err(CfgError::NotInit("no config keys available".to_string()))?
        }

        Ok(all)
    }

    pub async fn watch_config_changed<F, Fut>(&self, mut on_change: F) -> Result<(), CfgError>
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let config_center_client = self.config.as_ref().ok_or(CfgError::NotInit(
            "config center not configured".to_string(),
        ))?;

        let (config_changed_tx, mut config_changed_rx) = watch::channel(());
        let mut join_handles = Vec::new();
        for config_key in self
            .config_keys
            .as_ref()
            .ok_or(CfgError::NotInit("no config keys available".to_string()))?
        {
            if let Some(join_handle) = config_center_client
                .watch(&config_key, config_changed_tx.clone())
                .await
                .map_err(|e| CfgError::Init(e.to_string()))?
            {
                join_handles.push(join_handle);
            }
        }

        let join_handle = tokio::spawn(async move {
            info!("watch config changed (including common configs)...");
            loop {
                match config_changed_rx.changed().await {
                    Ok(_) => {
                        let _ = config_changed_rx.borrow().clone();
                        if let Err(e) = on_change().await {
                            warn!("handle config change error: {e:?}");
                        }
                    }
                    Err(err) => {
                        error!("watch config error: {:?}", err);
                        break;
                    }
                }
            }
        });
        join_handles.push(join_handle);

        self.add_join_handles(join_handles);
        Ok(())
    }

    fn snapshot_path(&self, config_key: &ConfigKey) -> Option<PathBuf> {
        self.snapshot_dir
            .clone()
            .map(|dir| dir.join(config_key.to_string()))
    }

    fn save_snapshot(&self, item: &ConfigItem) {
        if let Some(path) = self.snapshot_path(&item.key) {
            let snapshot = ConfigSnapshot {
                format: format!("{:?}", item.format),
                content: item.content.clone(),
            };
            if let Some(parent) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    warn!("failed to create snapshot dir: {:?}", e);
                    return;
                }
            }
            match serde_json::to_string(&snapshot) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&path, json) {
                        warn!("failed to write snapshot: {:?}", e);
                    } else {
                        info!("snapshot saved to {}", path.display());
                    }
                }
                Err(e) => warn!("failed to serialize snapshot: {:?}", e),
            }
        }
    }

    fn load_snapshot(&self, config_key: &ConfigKey) -> Option<ConfigItem> {
        let path = self.snapshot_path(config_key)?;
        let json = std::fs::read_to_string(&path)
            .inspect_err(|e| warn!("failed to read snapshot {}: {:?}", path.display(), e))
            .ok()?;
        let snapshot: ConfigSnapshot = serde_json::from_str(&json)
            .inspect_err(|e| warn!("failed to deserialize snapshot: {:?}", e))
            .ok()?;
        let format = parse_file_format(&snapshot.format)?;
        Some(ConfigItem {
            key: config_key.clone(),
            format,
            content: snapshot.content,
        })
    }

    pub async fn register(&self) -> Result<(), RegistryCenterError> {
        if let (Some(registry), Some(registry_key)) = (
            self.registry.as_ref().map(Arc::clone),
            self.registry_key.clone(),
        ) {
            let service_instance = build_service_instance(registry_key)?;
            self.service_instance.set(service_instance.clone()).ok();
            registry.register(&service_instance).await?;
        }
        Ok(())
    }

    pub async fn deregister(&self) -> Result<(), RegistryCenterError> {
        if let (Some(registry), Some(service_instance)) = (
            self.registry.as_ref().map(Arc::clone),
            self.service_instance.get().cloned(),
        ) {
            registry.deregister(&service_instance).await?;
        }
        Ok(())
    }

    pub async fn discover(
        &self,
        svc_name: &str,
    ) -> Result<Vec<ServiceInstance>, RegistryCenterError> {
        let registry = self
            .registry
            .as_ref()
            .ok_or(RegistryCenterError::BackendNotEnabled(
                "registry center not configured".to_string(),
            ))?;
        let namespace = self.registry_key.as_ref().and_then(|k| k.namespace.clone());
        let group = self.registry_key.as_ref().and_then(|k| k.group.clone());
        registry.discover(namespace, group, svc_name).await
    }

    fn add_join_handles(&self, handles: Vec<JoinHandle<()>>) {
        self.join_handles.lock().unwrap().extend(handles);
    }
}

fn build_service_instance(
    registry_key: RegistryKey,
) -> Result<ServiceInstance, RegistryCenterError> {
    let namespace = registry_key.namespace.clone();
    let group = registry_key.group.clone();
    let svc_name = registry_key.svc_name.clone();
    let ip = get_local_ip()?;
    let port = get_web_listen_port().ok_or(RegistryCenterError::WebServerNotRunning)?;
    let instance_id = format!("{svc_name}-{}-{port}", ip.replace('.', "-"));
    let health_check_url = get_health_check_url_http_protocol()
        .map(|prefix| format!("{}://{}:{}{}", prefix, ip, port, get_health_check_uri()));
    Ok(ServiceInstance {
        namespace,
        group,
        svc_name,
        instance_id,
        ip,
        port,
        health_check_url,
        metadata: Default::default(),
    })
}

fn parse_file_format(s: &str) -> Option<FileFormat> {
    match s {
        "Toml" => Some(FileFormat::Toml),
        "Json" => Some(FileFormat::Json),
        "Json5" => Some(FileFormat::Json5),
        "Yaml" => Some(FileFormat::Yaml),
        "Ini" => Some(FileFormat::Ini),
        "Ron" => Some(FileFormat::Ron),
        _ => None,
    }
}

/// 根据后端配置分支，统一构建 HubClient 所需的各个组件。
///
/// 使用泛型 `C` 统一处理不同后端的客户端类型，避免为 Consul/Etcd/Nacos 各写一套重复逻辑。
///
/// # 参数
/// - `hub_client`: 后端配置中的 HubClient 通用配置（base_url、namespace、group 等）
/// - `micro_svc_config`: 微服务全局配置，用于提取 svc_name、profile 等字段
/// - `config`: 配置中心相关配置（快照目录、文件格式、公共配置列表等）
/// - `client`: 已创建的后端客户端实例，若创建失败则为 `None`
///
/// # 返回
/// 返回一个元组，包含：
/// - 配置中心客户端 trait object
/// - 注册中心客户端 trait object
/// - 配置项的 ConfigKey 列表
/// - 快照目录路径
/// - 注册中心使用的 RegistryKey
fn build_branch<C: ConfigCenterClient + RegistryCenterClient + 'static>(
    hub_client_config: &HubClientConfig,
    micro_svc_config: &MicroSvcConfig,
    config_center_config: &Option<ConfigCenterConfig>,
    registry_center_config: &Option<RegistryCenterConfig>,
    hub_client: Option<C>,
) -> Result<
    (
        Option<Arc<dyn ConfigCenterClient>>,
        Option<Arc<dyn RegistryCenterClient>>,
        Option<Vec<ConfigKey>>,
        Option<PathBuf>,
        Option<RegistryKey>,
        Duration,
        Duration,
    ),
    CfgError,
> {
    let AppEnv { app_dir, .. } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;

    let svc_name = &micro_svc_config.svc_name.clone().unwrap(); // 服务名如果配置为空，在前面传进来的就会是应用名，这里不可能为空
    let profile = &micro_svc_config.profile;
    let namespace = hub_client_config.namespace.clone();
    let group = hub_client_config.group.clone().or_else(|| profile.clone());

    let registry_key = if let Some(_registry_center_config) = registry_center_config {
        Some(RegistryKey {
            namespace: namespace.clone(),
            group: group.clone(),
            svc_name: svc_name.clone(),
        })
    } else {
        None
    };

    let (snapshot_dir, config_keys) = if let Some(config_center_config) = config_center_config {
        let mut snapshot_dir = config_center_config.snapshot_dir.clone();
        // 如果是相对路径，相对的就是应用目录
        if snapshot_dir.is_relative() {
            snapshot_dir = app_dir.join(snapshot_dir);
        }

        let file_format = config_center_config.file_format.clone();
        // 构建公共配置项的 ConfigKey 列表
        let mut config_keys: Vec<ConfigKey> = config_center_config
            .common_configs
            .iter()
            .map(|data_id| ConfigKey::new(namespace.clone(), group.clone(), data_id.clone()))
            .collect();
        // 添加应用配置项的 ConfigKey
        let config_key = {
            let data_id = format!("{}.{}", svc_name, file_format);
            ConfigKey::new(namespace.clone(), group.clone(), data_id.clone())
        };
        config_keys.push(config_key);

        (Some(snapshot_dir), Some(config_keys))
    } else {
        (None, None)
    };

    let hub_client = hub_client.map(Arc::new);
    let config_center_client = hub_client.as_ref().map(|c| {
        let tmp: Arc<dyn ConfigCenterClient> = c.clone();
        tmp
    });
    let registry_center_client = hub_client.as_ref().map(|c| {
        let tmp: Arc<dyn RegistryCenterClient> = c.clone();
        tmp
    });
    let (retry_interval, refresh_interval) = registry_center_config
        .as_ref()
        .map(|c| (c.retry_interval, c.refresh_interval))
        .unwrap_or_default();
    Ok((
        config_center_client,
        registry_center_client,
        config_keys,
        snapshot_dir,
        registry_key,
        retry_interval,
        refresh_interval,
    ))
}
