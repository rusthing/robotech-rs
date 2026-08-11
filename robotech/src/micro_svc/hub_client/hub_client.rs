use crate::cfg::CfgError;
use crate::micro_svc::{
    ConfigCenterClient, ConsulClient, EtcdClient, MicroSvcConfig, NacosClient, RegistryCenterClient,
};
use std::sync::Arc;

pub struct HubClient {
    _config: Option<Arc<dyn ConfigCenterClient>>,
    _registry: Option<Arc<dyn RegistryCenterClient>>,
}

impl HubClient {
    pub async fn new(
        app_name: &str,
        profile: &Option<String>,
        micro_svc_config: MicroSvcConfig,
    ) -> Result<Self, CfgError> {
        let app_name = app_name.to_string();

        let (config_center_client, registry_center_client) =
            if let Some(consul_config) = micro_svc_config.clone().consul {
                let client = Arc::new(
                    ConsulClient::new(app_name, profile, micro_svc_config)
                        .map_err(|e| CfgError::Init(e.to_string()))?,
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
                    EtcdClient::new(app_name, profile, micro_svc_config)
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
                    NacosClient::new(app_name, micro_svc_config)
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
}
