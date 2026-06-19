use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct WebhookConfig {
    #[serde()]
    pub method: String,
    #[serde()]
    pub uri: String,
}
