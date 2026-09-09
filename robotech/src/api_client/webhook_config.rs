use crate::api_client::api_client_config::ApiAuthStrategy;
use serde::Deserialize;
use wheel_rs::urn_utils::Urn;

/// # Webhook 配置
///
/// 定义一个 webhook 的目标 URN 与可选的认证策略，供 `ApiClientUtils::webhook` 使用。
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct WebhookConfig {
    /// 目标 URN（格式: `方法:URL`，例如 `POST:http://127.0.0.1:8080/api/hook`）
    #[serde()]
    pub urn: Urn,
    /// 可选的认证策略
    #[serde(default)]
    pub auth: Option<ApiAuthStrategy>,
}
