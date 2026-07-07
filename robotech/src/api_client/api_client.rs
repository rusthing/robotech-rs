use crate::api_client::api_client_config::{ApiAuthStrategy, ApiClientConfig, Claim};
use crate::api_client::ApiClientError;
use crate::ro::Ro;
use chrono::Utc;
use http::header::HeaderMap;
use http::Method;
use jsonwebtoken::{encode, EncodingKey};
use reqwest::{Client, RequestBuilder, Response};
use robotech_macros::log_call;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::LazyLock;
use wheel_rs::urn_utils::Urn;

pub static REQWEST_CLIENT: LazyLock<Client> = LazyLock::new(|| Client::new());

#[derive(Debug, Clone)]
pub struct ApiClient {
    pub api_client_config: ApiClientConfig,
}

impl ApiClient {
    fn build_request<D: Serialize + ?Sized>(
        &self,
        method: Method,
        uri: &str,
        params: Option<&D>,
        body: Option<&D>,
        headers: Option<HeaderMap>,
        auth: Option<ApiAuthStrategy>,
    ) -> Result<(Urn, RequestBuilder), ApiClientError> {
        let url = format!("{}{}", self.api_client_config.base_url, uri);
        let urn = Urn::from_str(&format!("{method}:{url}"))
            .map_err(|e| ApiClientError::SetApiClient(format!("解析url失败: {e}")))?;
        tracing::debug!("request: {urn}....");
        let mut request_builder = REQWEST_CLIENT.request(method, &url);
        if let Some(headers) = headers {
            request_builder = request_builder.headers(headers);
        }
        if let Some(params) = params {
            request_builder = request_builder.query(params);
        }
        if let Some(body) = body {
            request_builder = request_builder.json(body);
        }

        if let Some(auth) = auth {
            match auth {
                ApiAuthStrategy::Token { header, token } => {
                    request_builder = request_builder.header(header, token);
                }
                ApiAuthStrategy::Basic { username, password } => {
                    request_builder = request_builder.basic_auth(username, password);
                }
                ApiAuthStrategy::Bearer {
                    algorithm,
                    private_key,
                    sub,
                    iss,
                    expires_in,
                } => {
                    let now = Utc::now();
                    let claim = Claim {
                        sub,
                        iss,
                        iat: now.timestamp(),
                        exp: (now + expires_in).timestamp(),
                    };
                    let token = encode(
                        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::from_str(
                            algorithm.as_str(),
                        )?),
                        &claim,
                        &EncodingKey::from_base64_secret(&private_key)?,
                    )?;
                    request_builder = request_builder.bearer_auth(token);
                }
            }
        }

        Ok((urn, request_builder))
    }

    async fn send(urn: &Urn, request_builder: RequestBuilder) -> Result<Response, ApiClientError> {
        let response = request_builder
            .send()
            .await
            .map_err(|e| ApiClientError::Request(urn.to_string(), e))?;
        tracing::debug!("{urn} response....");
        // 检查状态码，如果不是成功状态码则转换为错误
        let status_code = response.status();
        if !status_code.is_success() {
            return Err(ApiClientError::NonSuccessStatus(
                urn.to_string(),
                status_code.to_string(),
            ));
        }
        Ok(response)
    }

    async fn response_json<E>(urn: &Urn, response: Response) -> Result<Ro<E>, ApiClientError>
    where
        E: DeserializeOwned,
    {
        let response_text = response
            .text()
            .await
            .map_err(|e| ApiClientError::Response(urn.to_string(), e))?;
        tracing::debug!("{urn} response body: {response_text}");

        // 将文本解析为JSON
        let result: Ro<E> = serde_json::from_str(&response_text)
            .map_err(|e| ApiClientError::ParseJson(urn.to_string(), e))?;
        Ok(result)
    }

    /// 执行请求的通用方法
    #[log_call]
    pub async fn request<D, E>(
        &self,
        method: Method,
        uri: &str,
        params: Option<&D>,
        body: Option<&D>,
        headers: Option<HeaderMap>,
        auth: Option<ApiAuthStrategy>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        let (urn, request_builder) =
            self.build_request(method, uri, params, body, headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }

    /// 执行Webhook方法
    /// 根据请求方法智能识别data应该是params还是body
    /// GET方法为params，其它方法为body
    #[log_call]
    pub async fn webhook<D, E>(
        &self,
        method: Method,
        uri: &str,
        data: Option<&D>,
        headers: Option<HeaderMap>,
        auth: Option<ApiAuthStrategy>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        match method {
            Method::GET => self.request(method, uri, data, None, headers, auth).await,
            _ => self.request(method, uri, None, data, headers, auth).await,
        }
    }

    /// 执行GET请求的通用方法
    #[log_call]
    pub async fn get<D: Serialize + ?Sized + std::fmt::Debug>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<HeaderMap>,
        auth: Option<ApiAuthStrategy>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let (urn, request_builder) =
            self.build_request(Method::GET, uri, params, None, headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }

    /// 执行GET请求的通用方法，返回bytes
    #[log_call]
    pub async fn get_bytes<D: Serialize + ?Sized + std::fmt::Debug>(
        &self,
        uri: &str,
        params: Option<&D>,
        headers: Option<HeaderMap>,
        auth: Option<ApiAuthStrategy>,
    ) -> Result<Vec<u8>, ApiClientError> {
        let (urn, request_builder) =
            self.build_request(Method::GET, uri, params, None, headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        let result = response
            .bytes()
            .await
            .map_err(|e| ApiClientError::ParseBytes(urn.to_string(), e))?;
        tracing::debug!("{urn} response.");
        Ok(result.to_vec())
    }

    /// 执行POST请求的通用方法
    #[log_call]
    pub async fn post<D: Serialize + ?Sized + std::fmt::Debug>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<HeaderMap>,
        auth: Option<ApiAuthStrategy>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let (urn, request_builder) =
            self.build_request(Method::POST, uri, None, body, headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }
    /// 执行PUT请求的通用方法
    #[log_call]
    pub async fn put<D: Serialize + ?Sized + std::fmt::Debug>(
        &self,
        uri: &str,
        headers: Option<HeaderMap>,
        body: &D,
        auth: Option<ApiAuthStrategy>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let (urn, request_builder) =
            self.build_request(Method::PUT, uri, None, Some(body), headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }
    /// 执行DELETE请求的通用方法
    #[log_call]
    pub async fn delete<D: Serialize + ?Sized + std::fmt::Debug>(
        &self,
        uri: &str,
        body: Option<&D>,
        headers: Option<HeaderMap>,
        auth: Option<ApiAuthStrategy>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let (urn, request_builder) =
            self.build_request(Method::DELETE, uri, None, body, headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }

    /// 执行post multipart请求的通用方法
    #[log_call]
    pub async fn multipart(
        &self,
        uri: &str,
        form: reqwest::multipart::Form,
        headers: Option<HeaderMap>,
        auth: Option<ApiAuthStrategy>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let (urn, mut request_builder) =
            self.build_request::<String>(Method::POST, uri, None, None, headers, auth)?;
        request_builder = request_builder.multipart(form);
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }
}
