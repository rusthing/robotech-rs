use crate::cfg::CfgError;
use crate::micro_svc::{
    ConfigCenterClient, ConfigItem, ConsulClient, EtcdClient, MicroSvcConfig, NacosClient,
    RegistryCenterClient,
};
use arc_swap::ArcSwapOption;
use std::sync::Arc;
use tracing::info;

static HUB_CLIENT: ArcSwapOption<HubClient> = ArcSwapOption::const_empty();

pub async fn setup_hub_client(micro_svc_config: MicroSvcConfig) -> Result<(), CfgError> {
    info!("setup hub client...");
    let hub_client = HubClient::new(micro_svc_config).await?;
    HUB_CLIENT.store(Some(Arc::new(hub_client)));
    Ok(())
}

pub fn get_hub_client() -> Result<Arc<HubClient>, CfgError> {
    HUB_CLIENT
        .load_full()
        .ok_or(CfgError::NotInit("HUB_CLIENT not initialized".to_string()))
}

pub struct HubClient {
    _config: Option<Arc<dyn ConfigCenterClient>>,
    _registry: Option<Arc<dyn RegistryCenterClient>>,
}

impl HubClient {
    pub async fn new(micro_svc_config: MicroSvcConfig) -> Result<Self, CfgError> {
        let (config_center_client, registry_center_client) = if let Some(consul_config) =
            micro_svc_config.clone().consul
        {
            let client = Arc::new(
                ConsulClient::new(micro_svc_config).map_err(|e| CfgError::Init(e.to_string()))?,
            );
            let config_center_client = consul_config.config.map(|_| {
                let tmp: Arc<dyn ConfigCenterClient> = client.clone();
                tmp
            });
            let registry_center_client = consul_config.registry.map(|_| {
                let tmp: Arc<dyn RegistryCenterClient> = client.clone();
                tmp
            });
            (config_center_client, registry_center_client)
        } else if let Some(etcd_config) = micro_svc_config.clone().etcd {
            let client = Arc::new(
                EtcdClient::new(micro_svc_config)
                    .await
                    .map_err(|e| CfgError::Init(e.to_string()))?,
            );
            let config_center_client = etcd_config.config.map(|_| {
                let tmp: Arc<dyn ConfigCenterClient> = client.clone();
                tmp
            });
            let registry_center_client = etcd_config.registry.map(|_| {
                let tmp: Arc<dyn RegistryCenterClient> = client.clone();
                tmp
            });
            (config_center_client, registry_center_client)
        } else if let Some(nacos_config) = micro_svc_config.clone().nacos {
            let client = Arc::new(
                NacosClient::new(micro_svc_config)
                    .await
                    .map_err(|e| CfgError::Init(e.to_string()))?,
            );
            let config_center_client = nacos_config.config.map(|_| {
                let tmp: Arc<dyn ConfigCenterClient> = client.clone();
                tmp
            });
            let registry_center_client = nacos_config.registry.map(|_| {
                let tmp: Arc<dyn RegistryCenterClient> = client.clone();
                tmp
            });
            (config_center_client, registry_center_client)
        } else {
            (None, None)
        };
        Ok(Self {
            _config: config_center_client,
            _registry: registry_center_client,
        })
    }

    pub async fn get_config(&self) -> Result<Option<ConfigItem>, CfgError> {
        if let Some(config_center_client) = self._config.as_ref() {
            Ok(Some(
                config_center_client
                    .fetch()
                    .await
                    .map_err(|e| CfgError::Init(e.to_string()))?,
            ))
        } else {
            Ok(None)
        }
    }
}
