use crate::api::api_settings::ApiSettings;
use crate::cst::user_id_cst::USER_ID_HEADER_NAME;
use crate::ro::Ro;
use async_trait::async_trait;
use reqwest::Client;
use std::sync::LazyLock;

pub static REQWEST_CLIENT: LazyLock<Client> = LazyLock::new(|| Client::new());

#[async_trait]
pub trait BaseApi {
    fn get_api_settings(&self) -> &ApiSettings;

    /// 执行GET请求的通用方法
    async fn get(
        &self,
        path: &str,
        current_user_id: u64,
    ) -> Result<Ro<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request get: {}", url);
        let response = REQWEST_CLIENT
            .get(&url)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .send()
            .await?;
        let result = response.json().await?;
        Ok(result)
    }

    /// 执行GET请求的通用方法，返回bytes
    async fn get_bytes(
        &self,
        path: &str,
        current_user_id: u64,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request get: {}", url);
        let response = REQWEST_CLIENT
            .get(&url)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .send()
            .await?;
        let result = response.bytes().await?;
        Ok(result.to_vec())
    }

    /// 执行POST请求的通用方法
    async fn post<B: serde::Serialize + Sync>(
        &self,
        path: &str,
        body: &B,
        current_user_id: u64,
    ) -> Result<Ro<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request post: {}", url);
        let response = REQWEST_CLIENT
            .post(&url)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .json(body)
            .send()
            .await?;
        let ro = response.json().await?;
        Ok(ro)
    }
    /// 执行PUT请求的通用方法
    async fn put<B: serde::Serialize + Sync>(
        &self,
        path: &str,
        body: &B,
        current_user_id: u64,
    ) -> Result<Ro<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request put: {}", url);
        let response = REQWEST_CLIENT
            .put(&url)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .json(body)
            .send()
            .await?;
        let ro = response.json().await?;
        Ok(ro)
    }
    /// 执行DELETE请求的通用方法
    async fn delete<B: serde::Serialize>(
        &self,
        path: &str,
        current_user_id: u64,
    ) -> Result<Ro<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request delete: {}", url);
        let response = REQWEST_CLIENT
            .delete(&url)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .send()
            .await?;
        let ro = response.json().await?;
        Ok(ro)
    }
    /// 执行post multipart请求的通用方法
    async fn multipart(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
        current_user_id: u64,
    ) -> Result<Ro<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request post multipart: {}", url);
        let response = REQWEST_CLIENT
            .post(&url)
            .multipart(form)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .send()
            .await?;
        let ro = response.json().await?;
        Ok(ro)
    }
}
