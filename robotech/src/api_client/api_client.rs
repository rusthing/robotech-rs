use crate::api_client::ApiClientError;
use crate::ro::Ro;
use async_trait::async_trait;
use http::Method;
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;

#[async_trait]
pub trait ApiClient {
    async fn request<D, E>(
        &self,
        method: Method,
        uri: &str,
        params: Option<&D>,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug + Send + Sync,
        E: DeserializeOwned + Debug + Send;

    async fn webhook<D, E>(
        &self,
        method: Method,
        uri: &str,
        data: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug + Send + Sync,
        E: DeserializeOwned + Debug + Send,
    {
        match method {
            Method::GET => self.request(method, uri, data, None, headers).await,
            _ => self.request(method, uri, None, data, headers).await,
        }
    }

    async fn get<D: Serialize + ?Sized + Debug + Send + Sync>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError>;

    async fn get_bytes<D: Serialize + ?Sized + Debug + Send + Sync>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Vec<u8>, ApiClientError>;

    async fn post<D: Serialize + ?Sized + Debug + Send + Sync>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError>;

    async fn put<D: Serialize + ?Sized + Debug + Send + Sync>(
        &self,
        uri: &str,
        headers: Option<&HeaderMap>,
        body: &D,
    ) -> Result<Ro<serde_json::Value>, ApiClientError>;

    async fn delete<D: Serialize + ?Sized + Debug + Send + Sync>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError>;

    async fn multipart(
        &self,
        uri: &str,
        form: reqwest::multipart::Form,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError>;
}
