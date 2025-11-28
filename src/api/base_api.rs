use crate::api::api_settings::ApiSettings;
use crate::ro::Ro;
use async_trait::async_trait;
use reqwest::Client;
use std::sync::LazyLock;

pub static REQWEST_CLIENT: LazyLock<Client> = LazyLock::new(|| Client::new());

#[async_trait]
pub trait BaseApi {
    fn get_api_settings(&self) -> &ApiSettings;

    /// 执行GET请求的通用方法
    async fn get(&self, path: &str) -> Result<Ro<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request get: {}", url);
        let response = REQWEST_CLIENT.get(&url).send().await?;
        let result = response.json().await?;
        Ok(result)
    }

    /// 执行GET请求的通用方法，返回bytes
    async fn get_bytes(&self, path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request get: {}", url);
        let response = REQWEST_CLIENT.get(&url).send().await?;
        let result = response.bytes().await?;
        Ok(result.to_vec())
    }

    /// 执行POST请求的通用方法
    async fn post<B: serde::Serialize + Sync>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<Ro<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request post: {}", url);
        let response = REQWEST_CLIENT.post(&url).json(body).send().await?;
        let ro = response.json().await?;
        Ok(ro)
    }
    /// 执行PUT请求的通用方法
    async fn put<B: serde::Serialize + Sync>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<Ro<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request put: {}", url);
        let response = REQWEST_CLIENT.put(&url).json(body).send().await?;
        let ro = response.json().await?;
        Ok(ro)
    }
    /// 执行DELETE请求的通用方法
    async fn delete<B: serde::Serialize>(
        &self,
        path: &str,
    ) -> Result<Ro<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request delete: {}", url);
        let response = REQWEST_CLIENT.delete(&url).send().await?;
        let ro = response.json().await?;
        Ok(ro)
    }
    /// 执行post multipart请求的通用方法
    async fn multipart(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<Ro<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!("{}{}", self.get_api_settings().base_url, path);
        log::debug!("request post multipart: {}", url);
        let response = REQWEST_CLIENT.post(&url).multipart(form).send().await?;
        let ro = response.json().await?;
        Ok(ro)
    }
}
