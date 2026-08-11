use serde::{Deserialize, Serialize};
use wheel_rs::serde::vec_serde;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct HubClientConfig {
    /// 服务器基础URL
    #[serde(with = "vec_serde")]
    pub base_url: Vec<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}
