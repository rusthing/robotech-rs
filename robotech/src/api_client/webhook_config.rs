use crate::api_client::api_client_config::ApiAuthStrategy;
use serde::Deserialize;
use wheel_rs::urn_utils::Urn;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct WebhookConfig {
    #[serde()]
    pub urn: Urn,
    #[serde(default)]
    pub auth: Option<ApiAuthStrategy>,
}
