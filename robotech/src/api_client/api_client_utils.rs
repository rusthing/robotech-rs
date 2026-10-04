use crate::api_client::api_client_config::{ApiAuthStrategy, Claim};
use crate::api_client::ApiClientError;
use crate::api::Ro;
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

/// # 全局 HTTP 客户端
///
/// 基于 reqwest 的全局复用客户端，供所有 API 请求使用。
pub static REQWEST_CLIENT: LazyLock<Client> = LazyLock::new(Client::new);

/// # API 客户端工具
///
/// 提供基于 URN 约定的 GET/POST/PUT/DELETE、multipart 与 webhook 等请求封装，
/// 统一返回 `Ro<E>` 结构；支持 Token、Basic、Bearer 三种认证策略。
#[derive(Debug, Clone)]
pub struct ApiClientUtils;

impl ApiClientUtils {
    /// 构建请求（组装 URL、headers、params、body 与认证信息）
    fn build_request<D: Serialize + ?Sized>(
        method: Method,
        base_url: &str,
        uri: &str,
        params: Option<&D>,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
        auth: Option<&ApiAuthStrategy>,
    ) -> Result<(Urn, RequestBuilder), ApiClientError> {
        let url = format!("{}{}", base_url, uri);
        let urn = Urn::from_str(&format!("{method}:{url}"))
            .map_err(|e| ApiClientError::SetApiClient(format!("解析url失败: {e}")))?;
        tracing::debug!("request: {urn}....");
        let mut request_builder = REQWEST_CLIENT.request(method, &url);
        if let Some(headers) = headers {
            request_builder = request_builder.headers(headers.clone());
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
                    request_builder = request_builder.basic_auth(username, password.clone());
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
                        sub: sub.clone(),
                        iss: iss.clone(),
                        iat: now.timestamp(),
                        exp: (now + *expires_in).timestamp(),
                    };
                    let token = encode(
                        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::from_str(
                            algorithm.as_str(),
                        )?),
                        &claim,
                        &EncodingKey::from_base64_secret(private_key)?,
                    )?;
                    request_builder = request_builder.bearer_auth(token);
                }
            }
        }

        Ok((urn, request_builder))
    }

    /// 发送请求并检查状态码
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

    /// 将响应体解析为 `Ro<E>` JSON 对象
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

    /// # 执行请求的通用方法
    ///
    /// 构建、发送请求并将响应解析为 `Ro<E>`。
    ///
    /// ## 参数
    /// * `method` - HTTP 方法
    /// * `base_url` - 基础 URL
    /// * `uri` - 请求 URI
    /// * `params` - 可选的查询参数
    /// * `body` - 可选的请求体
    /// * `headers` - 可选的请求头
    /// * `auth` - 可选的认证策略
    ///
    /// ## 返回值
    /// 返回 `Ok(Ro<E>)`；请求或解析失败时返回 `Err(ApiClientError)`
    #[log_call]
    pub async fn request<D, E>(
        method: Method,
        base_url: &str,
        uri: &str,
        params: Option<&D>,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
        auth: Option<&ApiAuthStrategy>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        let (urn, request_builder) =
            Self::build_request(method, base_url, uri, params, body, headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }

    /// # 执行 Webhook 请求
    ///
    /// 根据请求方法智能识别 `data` 应该作为 params 还是 body：
    /// GET 方法为 params，其它方法为 body。
    ///
    /// ## 参数
    /// * `method` - HTTP 方法
    /// * `base_url` - 基础 URL
    /// * `uri` - 请求 URI
    /// * `data` - 可选的请求数据（params 或 body）
    /// * `headers` - 可选的请求头
    /// * `auth` - 可选的认证策略
    ///
    /// ## 返回值
    /// 返回 `Ok(Ro<E>)`；请求或解析失败时返回 `Err(ApiClientError)`
    #[log_call]
    pub async fn webhook<D, E>(
        method: Method,
        base_url: &str,
        uri: &str,
        data: Option<&D>,
        headers: Option<&HeaderMap>,
        auth: Option<&ApiAuthStrategy>,
    ) -> Result<Ro<E>, ApiClientError>
    where
        D: Serialize + ?Sized + Debug,
        E: DeserializeOwned + Debug,
    {
        match method {
            Method::GET => Self::request(method, base_url, uri, data, None, headers, auth).await,
            _ => Self::request(method, base_url, uri, None, data, headers, auth).await,
        }
    }

    /// # 执行 GET 请求
    ///
    /// ## 参数
    /// * `base_url` - 基础 URL
    /// * `uri` - 请求 URI
    /// * `params` - 可选的查询参数
    /// * `headers` - 可选的请求头
    /// * `auth` - 可选的认证策略
    ///
    /// ## 返回值
    /// 返回 `Ok(Ro<serde_json::Value>)`；请求或解析失败时返回 `Err(ApiClientError)`
    #[log_call]
    pub async fn get<D: Serialize + ?Sized + std::fmt::Debug>(
        base_url: &str,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
        auth: Option<&ApiAuthStrategy>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let (urn, request_builder) =
            Self::build_request(Method::GET, base_url, uri, params, None, headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }

    /// # 执行 GET 请求并返回字节流
    ///
    /// ## 参数
    /// * `base_url` - 基础 URL
    /// * `uri` - 请求 URI
    /// * `params` - 可选的查询参数
    /// * `headers` - 可选的请求头
    /// * `auth` - 可选的认证策略
    ///
    /// ## 返回值
    /// 返回 `Ok(Vec<u8>)`；请求或解析失败时返回 `Err(ApiClientError)`
    #[log_call]
    pub async fn get_bytes<D: Serialize + ?Sized + std::fmt::Debug>(
        base_url: &str,
        uri: &str,
        params: Option<&D>,
        headers: Option<&HeaderMap>,
        auth: Option<&ApiAuthStrategy>,
    ) -> Result<Vec<u8>, ApiClientError> {
        let (urn, request_builder) =
            Self::build_request(Method::GET, base_url, uri, params, None, headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        let result = response
            .bytes()
            .await
            .map_err(|e| ApiClientError::ParseBytes(urn.to_string(), e))?;
        tracing::debug!("{urn} response.");
        Ok(result.to_vec())
    }

    /// # 执行 POST 请求
    ///
    /// ## 参数
    /// * `base_url` - 基础 URL
    /// * `uri` - 请求 URI
    /// * `body` - 可选的请求体
    /// * `headers` - 可选的请求头
    /// * `auth` - 可选的认证策略
    ///
    /// ## 返回值
    /// 返回 `Ok(Ro<serde_json::Value>)`；请求或解析失败时返回 `Err(ApiClientError)`
    #[log_call]
    pub async fn post<D: Serialize + ?Sized + std::fmt::Debug>(
        base_url: &str,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
        auth: Option<&ApiAuthStrategy>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let (urn, request_builder) =
            Self::build_request(Method::POST, base_url, uri, None, body, headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }
    /// # 执行 PUT 请求
    ///
    /// ## 参数
    /// * `base_url` - 基础 URL
    /// * `uri` - 请求 URI
    /// * `headers` - 可选的请求头
    /// * `body` - 请求体
    /// * `auth` - 可选的认证策略
    ///
    /// ## 返回值
    /// 返回 `Ok(Ro<serde_json::Value>)`；请求或解析失败时返回 `Err(ApiClientError)`
    #[log_call]
    pub async fn put<D: Serialize + ?Sized + std::fmt::Debug>(
        base_url: &str,
        uri: &str,
        headers: Option<&HeaderMap>,
        body: &D,
        auth: Option<&ApiAuthStrategy>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let (urn, request_builder) =
            Self::build_request(Method::PUT, base_url, uri, None, Some(body), headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }
    /// # 执行 DELETE 请求
    ///
    /// ## 参数
    /// * `base_url` - 基础 URL
    /// * `uri` - 请求 URI
    /// * `body` - 可选的请求体
    /// * `headers` - 可选的请求头
    /// * `auth` - 可选的认证策略
    ///
    /// ## 返回值
    /// 返回 `Ok(Ro<serde_json::Value>)`；请求或解析失败时返回 `Err(ApiClientError)`
    #[log_call]
    pub async fn delete<D: Serialize + ?Sized + std::fmt::Debug>(
        base_url: &str,
        uri: &str,
        body: Option<&D>,
        headers: Option<&HeaderMap>,
        auth: Option<&ApiAuthStrategy>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let (urn, request_builder) =
            Self::build_request(Method::DELETE, base_url, uri, None, body, headers, auth)?;
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }

    /// # 执行 POST multipart 请求
    ///
    /// ## 参数
    /// * `base_url` - 基础 URL
    /// * `uri` - 请求 URI
    /// * `form` - multipart 表单
    /// * `headers` - 可选的请求头
    /// * `auth` - 可选的认证策略
    ///
    /// ## 返回值
    /// 返回 `Ok(Ro<serde_json::Value>)`；请求或解析失败时返回 `Err(ApiClientError)`
    #[log_call]
    pub async fn multipart(
        base_url: &str,
        uri: &str,
        form: reqwest::multipart::Form,
        headers: Option<&HeaderMap>,
        auth: Option<&ApiAuthStrategy>,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let (urn, mut request_builder) =
            Self::build_request::<String>(Method::POST, base_url, uri, None, None, headers, auth)?;
        request_builder = request_builder.multipart(form);
        let response = Self::send(&urn, request_builder).await?;
        Self::response_json(&urn, response).await
    }
}
