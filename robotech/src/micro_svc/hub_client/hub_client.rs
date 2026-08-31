use crate::cfg::CfgError;
use crate::env::APP_ENV;
use crate::micro_svc::{
    ConfigCenterClient, ConfigItem, ConfigKey, ConsulClient, EtcdClient, MicroSvcConfig,
    NacosClient, RegistryCenterClient, RegistryCenterError, ServiceInstance,
};
use arc_swap::ArcSwapOption;
use config::FileFormat;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::{error, info, warn};

static HUB_CLIENT: ArcSwapOption<HubClient> = ArcSwapOption::const_empty();

pub async fn setup_hub_client(micro_svc_config: MicroSvcConfig) -> Result<(), CfgError> {
    info!("setup hub client...: {micro_svc_config:?}");
    let hub_client = HubClient::new(micro_svc_config).await?;
    HUB_CLIENT.store(Some(Arc::new(hub_client)));
    Ok(())
}

fn get_hub_client() -> Result<Arc<HubClient>, CfgError> {
    HUB_CLIENT
        .load_full()
        .ok_or(CfgError::NotInit("HUB_CLIENT not initialized".to_string()))
}

pub async fn get_config() -> Result<Option<Vec<ConfigItem>>, CfgError> {
    match get_hub_client() {
        Ok(hub_client) => {
            let config = hub_client.get_configs().await?;
            info!("get config center config: {:?}", config);
            Ok(config)
        }
        Err(e) => {
            error!("get config failed: {:?}", e);
            Ok(None)
        }
    }
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

pub async fn register(instance: &ServiceInstance) -> Result<(), RegistryCenterError> {
    let hub_client =
        get_hub_client().map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
    hub_client.register(instance).await
}

pub async fn deregister(instance_id: &str) -> Result<(), RegistryCenterError> {
    let hub_client =
        get_hub_client().map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
    hub_client.deregister(instance_id).await
}

pub async fn discover(service_name: &str) -> Result<Vec<ServiceInstance>, RegistryCenterError> {
    let hub_client =
        get_hub_client().map_err(|e| RegistryCenterError::Connection(e.to_string()))?;
    hub_client.discover(service_name).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigSnapshot {
    format: String,
    content: String,
}

pub struct HubClient {
    config: Option<Arc<dyn ConfigCenterClient>>,
    registry: Option<Arc<dyn RegistryCenterClient>>,
    config_key: Option<ConfigKey>,
    snapshot_dir: Option<PathBuf>,
    common_config_keys: Vec<ConfigKey>,
    config_watch_join_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Drop for HubClient {
    fn drop(&mut self) {
        if let Some(handle) = self.config_watch_join_handle.lock().unwrap().take() {
            handle.abort();
        }
    }
}

impl HubClient {
    pub async fn new(micro_svc_config: MicroSvcConfig) -> Result<Self, CfgError> {
        let (
            config_center_client,
            registry_center_client,
            config_key,
            snapshot_dir,
            common_config_keys,
        ) = if let Some(consul_config) = micro_svc_config.clone().consul {
            let snapshot_dir = consul_config
                .config
                .as_ref()
                .map(|c| c.snapshot_dir.clone());
            let config_key = consul_config.config.as_ref().and_then(|c| {
                build_config_key(
                    &micro_svc_config.svc_name,
                    &micro_svc_config.profile,
                    &consul_config.hub_client.namespace,
                    &consul_config.hub_client.group,
                    &c.file_format,
                )
            });
            let common_config_keys = consul_config
                .config
                .as_ref()
                .map(|c| {
                    build_common_config_keys(
                        &c.common_configs,
                        &consul_config.hub_client.namespace,
                        &consul_config.hub_client.group,
                        &micro_svc_config.profile,
                    )
                })
                .unwrap_or_default();
            let client = ConsulClient::new(micro_svc_config)
                .map(Arc::new)
                .map_err(|e| {
                    warn!(
                        "failed to create consul client: {:?}, will use snapshot if available",
                        e
                    );
                    e
                })
                .ok();
            let config_center_client = client.as_ref().and_then(|c| {
                consul_config.config.map(|_| {
                    let tmp: Arc<dyn ConfigCenterClient> = c.clone();
                    tmp
                })
            });
            let registry_center_client = client.as_ref().and_then(|c| {
                consul_config.registry.map(|_| {
                    let tmp: Arc<dyn RegistryCenterClient> = c.clone();
                    tmp
                })
            });
            (
                config_center_client,
                registry_center_client,
                config_key,
                snapshot_dir,
                common_config_keys,
            )
        } else if let Some(etcd_config) = micro_svc_config.clone().etcd {
            let snapshot_dir = etcd_config.config.as_ref().map(|c| c.snapshot_dir.clone());
            let config_key = etcd_config.config.as_ref().and_then(|c| {
                build_config_key(
                    &micro_svc_config.svc_name,
                    &micro_svc_config.profile,
                    &etcd_config.hub_client.namespace,
                    &etcd_config.hub_client.group,
                    &c.file_format,
                )
            });
            let common_config_keys = etcd_config
                .config
                .as_ref()
                .map(|c| {
                    build_common_config_keys(
                        &c.common_configs,
                        &etcd_config.hub_client.namespace,
                        &etcd_config.hub_client.group,
                        &micro_svc_config.profile,
                    )
                })
                .unwrap_or_default();
            let client = EtcdClient::new(micro_svc_config)
                .await
                .map(Arc::new)
                .map_err(|e| {
                    warn!(
                        "failed to create etcd client: {:?}, will use snapshot if available",
                        e
                    );
                    e
                })
                .ok();
            let config_center_client = client.as_ref().and_then(|c| {
                etcd_config.config.map(|_| {
                    let tmp: Arc<dyn ConfigCenterClient> = c.clone();
                    tmp
                })
            });
            let registry_center_client = client.as_ref().and_then(|c| {
                etcd_config.registry.map(|_| {
                    let tmp: Arc<dyn RegistryCenterClient> = c.clone();
                    tmp
                })
            });
            (
                config_center_client,
                registry_center_client,
                config_key,
                snapshot_dir,
                common_config_keys,
            )
        } else if let Some(nacos_config) = micro_svc_config.clone().nacos {
            let snapshot_dir = nacos_config.config.as_ref().map(|c| c.snapshot_dir.clone());
            let config_key = nacos_config.config.as_ref().and_then(|c| {
                let namespace = nacos_config
                    .hub_client
                    .namespace
                    .clone()
                    .or(Some("public".to_string()));
                let group = nacos_config
                    .hub_client
                    .group
                    .clone()
                    .or_else(|| micro_svc_config.profile.clone())
                    .or(Some("DEFAULT_GROUP".to_string()));
                build_config_key(
                    &micro_svc_config.svc_name,
                    &None,
                    &namespace,
                    &group,
                    &c.file_format,
                )
            });
            let nacos_namespace = nacos_config
                .hub_client
                .namespace
                .clone()
                .or(Some("public".to_string()));
            let nacos_group = nacos_config
                .hub_client
                .group
                .clone()
                .or_else(|| micro_svc_config.profile.clone())
                .or(Some("DEFAULT_GROUP".to_string()));
            let common_config_keys = nacos_config
                .config
                .as_ref()
                .map(|c| {
                    build_common_config_keys(
                        &c.common_configs,
                        &nacos_namespace,
                        &nacos_group,
                        &None,
                    )
                })
                .unwrap_or_default();
            let client = NacosClient::new(micro_svc_config)
                .await
                .map(Arc::new)
                .map_err(|e| {
                    warn!(
                        "failed to create nacos client: {:?}, will use snapshot if available",
                        e
                    );
                    e
                })
                .ok();
            let config_center_client = client.as_ref().and_then(|c| {
                nacos_config.config.map(|_| {
                    let tmp: Arc<dyn ConfigCenterClient> = c.clone();
                    tmp
                })
            });
            let registry_center_client = client.as_ref().and_then(|c| {
                nacos_config.registry.map(|_| {
                    let tmp: Arc<dyn RegistryCenterClient> = c.clone();
                    tmp
                })
            });
            (
                config_center_client,
                registry_center_client,
                config_key,
                snapshot_dir,
                common_config_keys,
            )
        } else {
            (None, None, None, None, Vec::new())
        };
        Ok(Self {
            config: config_center_client,
            registry: registry_center_client,
            config_key,
            snapshot_dir,
            common_config_keys,
            config_watch_join_handle: Mutex::new(None),
        })
    }

    pub async fn get_configs(&self) -> Result<Option<Vec<ConfigItem>>, CfgError> {
        let config_center_client = match self.config.as_ref() {
            Some(client) => client,
            None => {
                if let Some(key) = &self.config_key {
                    warn!(
                        "config center client not available, trying snapshot for key: {}",
                        key
                    );
                    if let Some(item) = self.load_snapshot(key) {
                        warn!("using snapshot config (no backend connection)");
                        return Ok(Some(vec![item]));
                    }
                }
                return Ok(None);
            }
        };

        let mut all = Vec::new();

        let config_key = config_center_client
            .config_key()
            .map_err(|e| CfgError::Init(e.to_string()))?;
        let all_keys: Vec<&ConfigKey> = self
            .common_config_keys
            .iter()
            .chain(std::iter::once(&config_key))
            .collect();

        info!("fetching {} configs: {:?}", all_keys.len(), all_keys);
        for key in &all_keys {
            match config_center_client.fetch(key).await {
                Ok(item) => {
                    info!("loaded config: {}", key);
                    self.save_snapshot(&item);
                    all.push(item);
                }
                Err(e) => {
                    warn!("failed to fetch config {}: {:?}, trying snapshot", key, e);
                    if let Some(item) = self.load_snapshot(key) {
                        warn!("using snapshot for config: {}", key);
                        all.push(item);
                    } else {
                        return Err(CfgError::Init(e.to_string()));
                    }
                }
            }
        }
        Ok(Some(all))
    }

    fn snapshot_dir(&self) -> Option<PathBuf> {
        self.snapshot_dir.as_ref().map(|dir| {
            if dir.is_relative() {
                if let Some(app_env) = APP_ENV.get() {
                    app_env.app_dir.join(dir)
                } else {
                    dir.clone()
                }
            } else {
                dir.clone()
            }
        })
    }

    fn snapshot_path(&self, key: &ConfigKey) -> Option<PathBuf> {
        self.snapshot_dir()
            .map(|dir| dir.join(format!("{key}.json")))
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

    fn load_snapshot(&self, key: &ConfigKey) -> Option<ConfigItem> {
        let path = self.snapshot_path(key)?;
        let json = std::fs::read_to_string(&path)
            .inspect_err(|e| warn!("failed to read snapshot {}: {:?}", path.display(), e))
            .ok()?;
        let snapshot: ConfigSnapshot = serde_json::from_str(&json)
            .inspect_err(|e| warn!("failed to deserialize snapshot: {:?}", e))
            .ok()?;
        let format = parse_file_format(&snapshot.format)?;
        Some(ConfigItem {
            key: key.clone(),
            format,
            content: snapshot.content,
        })
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

        let config_key = config_center_client
            .config_key()
            .map_err(|e| CfgError::Init(e.to_string()))?;
        let all_keys: Vec<ConfigKey> = self
            .common_config_keys
            .iter()
            .cloned()
            .chain(std::iter::once(config_key))
            .collect();

        config_center_client
            .watch(&all_keys, config_changed_tx.clone())
            .await
            .map_err(|e| CfgError::Init(e.to_string()))?;

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

        *self.config_watch_join_handle.lock().unwrap() = Some(join_handle);
        Ok(())
    }

    pub async fn register(&self, instance: &ServiceInstance) -> Result<(), RegistryCenterError> {
        let registry = self.registry.as_ref().ok_or_else(|| {
            RegistryCenterError::Connection("registry center not configured".to_string())
        })?;
        registry.register(instance).await
    }

    pub async fn deregister(&self, instance_id: &str) -> Result<(), RegistryCenterError> {
        let registry = self.registry.as_ref().ok_or_else(|| {
            RegistryCenterError::Connection("registry center not configured".to_string())
        })?;
        registry.deregister(instance_id).await
    }

    pub async fn discover(
        &self,
        service_name: &str,
    ) -> Result<Vec<ServiceInstance>, RegistryCenterError> {
        let registry = self.registry.as_ref().ok_or_else(|| {
            RegistryCenterError::Connection("registry center not configured".to_string())
        })?;
        registry.discover(service_name).await
    }
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

fn build_config_key(
    svc_name: &Option<String>,
    profile: &Option<String>,
    namespace: &Option<String>,
    group: &Option<String>,
    file_format: &str,
) -> Option<ConfigKey> {
    let group = group.clone().or_else(|| profile.clone());
    let data_id = svc_name.clone()?;
    let data_id = format!("{}.{}", data_id, file_format);
    Some(ConfigKey::new(namespace.clone(), group, data_id))
}

fn build_common_config_keys(
    common_configs: &[String],
    namespace: &Option<String>,
    group: &Option<String>,
    profile: &Option<String>,
) -> Vec<ConfigKey> {
    let effective_group = group.clone().or_else(|| profile.clone());
    common_configs
        .iter()
        .map(|data_id| ConfigKey::new(namespace.clone(), effective_group.clone(), data_id.clone()))
        .collect()
}
