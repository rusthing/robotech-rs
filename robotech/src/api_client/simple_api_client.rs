use crate::api_client::ApiAuthStrategy;
use crate::api_client::ApiClientError;
use crate::api_client::ApiClientUtils;
use crate::api_client::ApiClient;
use crate::ro::Ro;
use async_trait::async_trait;
use http::Method;
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;

pub struct SimpleApiClient {
    base_url: String,
    auth: Option<ApiAuthStrategy>,
}

impl SimpleApiClient {
    pub fn new(base_url: String, auth: Option<ApiAuthStrategy>) -> Self {
        Self { base_url, auth }
    }

    pub async fn request<D, E>(
        &self,
        method: Method,
        uri: &str,
        params: Option<&D>,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        ApiClientUtils::request(
            method,
            &self.base_url,
            uri,
            params,
            body,
            headers,
            self.auth.as_ref(),
        )
        .await
    }

    pub async fn webhook<D, E>(
        &self,
        method: Method,
        uri: &str,
        data: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        ApiClientUtils::webhook(
            method,
            &self.base_url,
            uri,
            data,
            headers,
            self.auth.as_ref(),
        )
        .await
    }

    pub async fn get<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        ApiClientUtils::get(&self.base_url, uri, params, headers, self.auth.as_ref()).await
    }

    pub async fn get_bytes<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Vec<u8>, ApiClientError> {
        ApiClientUtils::get_bytes(&self.base_url, uri, params, headers, self.auth.as_ref()).await
    }

    pub async fn post<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        ApiClientUtils::post(&self.base_url, uri, body, headers, self.auth.as_ref()).await
    }

    pub async fn put<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        headers: Option<&HeaderMap>,
        body: &D,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        ApiClientUtils::put(&self.base_url, uri, headers, body, self.auth.as_ref()).await
    }

    pub async fn delete<D: Serialize + ?Sized + Debug>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        ApiClientUtils::delete(&self.base_url, uri, body, headers, self.auth.as_ref()).await
    }

    pub async fn multipart(
        &self,
        uri: &str,
        form: reqwest::multipart::Form,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        ApiClientUtils::multipart(&self.base_url, uri, form, headers, self.auth.as_ref()).await
    }
}

#[async_trait]
impl ApiClient for SimpleApiClient {
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
        E: DeserializeOwned + Debug + Send,
    {
        SimpleApiClient::request(self, method, uri, params, body, headers).await
    }

    async fn get<D: Serialize + ?Sized + Debug + Send + Sync>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        SimpleApiClient::get(self, uri, params, headers).await
    }

    async fn get_bytes<D: Serialize + ?Sized + Debug + Send + Sync>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Vec<u8>, ApiClientError> {
        SimpleApiClient::get_bytes(self, uri, params, headers).await
    }

    async fn post<D: Serialize + ?Sized + Debug + Send + Sync>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        SimpleApiClient::post(self, uri, body, headers).await
    }

    async fn put<D: Serialize + ?Sized + Debug + Send + Sync>(
        &self,
        uri: &str,
        headers: Option<&HeaderMap>,
        body: &D,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        SimpleApiClient::put(self, uri, headers, body).await
    }

    async fn delete<D: Serialize + ?Sized + Debug + Send + Sync>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        SimpleApiClient::delete(self, uri, body, headers).await
    }

    async fn multipart(
        &self,
        uri: &str,
        form: reqwest::multipart::Form,
        headers: Option<&HeaderMap>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        SimpleApiClient::multipart(self, uri, form, headers).await
    }
}