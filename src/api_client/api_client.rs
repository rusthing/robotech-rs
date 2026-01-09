use crate::api_client::api_client_config::ApiClientConfig;
use crate::api_client::ApiClientError;
use crate::api_client::ApiClientError::{
    BytesParseError, JsonParseError, RequestError, ResponseError, ResponseStatusError,
};
use crate::cst::user_id_cst::USER_ID_HEADER_NAME;
use crate::ro::Ro;
use reqwest::Client;
use std::sync::LazyLock;

pub static REQWEST_CLIENT: LazyLock<Client> = LazyLock::new(|| Client::new());

#[derive(Debug)]
pub struct CrudApiClient {
    pub api_client_config: ApiClientConfig,
}

impl CrudApiClient {
    /// 执行GET请求的通用方法
    pub async fn get(
        &self,
        path: &str,
        current_user_id: u64,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let url = format!("{}{}", self.api_client_config.base_url, path);
        let urn = format!("GET:{}", url);
        log::debug!("{}....", urn);
        let response = REQWEST_CLIENT
            .get(&url)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .send()
            .await
            .map_err(|e| RequestError(urn.clone(), e))?;
        log::debug!("{} response....", urn);
        // 检查状态码，如果不是成功状态码则转换为错误
        let status_code = response.status();
        if !status_code.is_success() {
            return Err(ResponseStatusError(url.clone(), status_code.to_string()));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| ResponseError(url.clone(), e))?;
        log::debug!("{} response body: {}", urn, response_text);

        // 将文本解析为JSON
        let result: Ro<serde_json::Value> =
            serde_json::from_str(&response_text).map_err(|e| JsonParseError(url, e))?;
        Ok(result)
    }

    /// 执行GET请求的通用方法，返回bytes
    pub async fn get_bytes(
        &self,
        path: &str,
        current_user_id: u64,
    ) -> Result<Vec<u8>, ApiClientError> {
        let url = format!("{}{}", self.api_client_config.base_url, path);
        let urn = format!("GET:{}", url);
        log::debug!("{}....", urn);
        let response = REQWEST_CLIENT
            .get(&url)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .send()
            .await
            .map_err(|e| RequestError(urn.clone(), e))?;
        log::debug!("{} response....", urn);
        // 检查状态码，如果不是成功状态码则转换为错误
        let status_code = response.status();
        if !status_code.is_success() {
            return Err(ResponseStatusError(url.clone(), status_code.to_string()));
        }

        let result = response
            .bytes()
            .await
            .map_err(|e| BytesParseError(urn.clone(), e))?;
        log::debug!("{} response.", urn);
        Ok(result.to_vec())
    }

    /// 执行POST请求的通用方法
    pub async fn post<B: serde::Serialize + Sync>(
        &self,
        path: &str,
        body: &B,
        current_user_id: u64,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let url = format!("{}{}", self.api_client_config.base_url, path);
        let urn = format!("POST:{}", url);
        log::debug!("{}....", urn);
        let response = REQWEST_CLIENT
            .post(&url)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .json(body)
            .send()
            .await
            .map_err(|e| RequestError(urn.clone(), e))?;
        log::debug!("{} response....", urn);
        // 检查状态码，如果不是成功状态码则转换为错误
        let status_code = response.status();
        if !status_code.is_success() {
            return Err(ResponseStatusError(url.clone(), status_code.to_string()));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| ResponseError(url.clone(), e))?;
        log::debug!("{} response body: {}", urn, response_text);

        // 将文本解析为JSON
        let result: Ro<serde_json::Value> =
            serde_json::from_str(&response_text).map_err(|e| JsonParseError(url, e))?;
        Ok(result)
    }
    /// 执行PUT请求的通用方法
    pub async fn put<B: serde::Serialize + Sync>(
        &self,
        path: &str,
        body: &B,
        current_user_id: u64,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let url = format!("{}{}", self.api_client_config.base_url, path);
        let urn = format!("PUT:{}", url);
        log::debug!("{}....", urn);
        let response = REQWEST_CLIENT
            .put(&url)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .json(body)
            .send()
            .await
            .map_err(|e| RequestError(urn.clone(), e))?;
        log::debug!("{} response....", urn);
        // 检查状态码，如果不是成功状态码则转换为错误
        let status_code = response.status();
        if !status_code.is_success() {
            return Err(ResponseStatusError(url.clone(), status_code.to_string()));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| ResponseError(url.clone(), e))?;
        log::debug!("{} response body: {}", urn, response_text);

        // 将文本解析为JSON
        let result: Ro<serde_json::Value> =
            serde_json::from_str(&response_text).map_err(|e| JsonParseError(url, e))?;
        Ok(result)
    }
    /// 执行DELETE请求的通用方法
    pub async fn delete<B: serde::Serialize>(
        &self,
        path: &str,
        current_user_id: u64,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let url = format!("{}{}", self.api_client_config.base_url, path);
        let urn = format!("DELETE:{}", url);
        log::debug!("{}....", urn);
        let response = REQWEST_CLIENT
            .delete(&url)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .send()
            .await
            .map_err(|e| RequestError(urn.clone(), e))?;
        log::debug!("{} response....", urn);
        // 检查状态码，如果不是成功状态码则转换为错误
        let status_code = response.status();
        if !status_code.is_success() {
            return Err(ResponseStatusError(url.clone(), status_code.to_string()));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| ResponseError(url.clone(), e))?;
        log::debug!("{} response body: {}", urn, response_text);

        // 将文本解析为JSON
        let result: Ro<serde_json::Value> =
            serde_json::from_str(&response_text).map_err(|e| JsonParseError(url, e))?;
        Ok(result)
    }
    /// 执行post multipart请求的通用方法
    pub async fn multipart(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
        current_user_id: u64,
    ) -> Result<Ro<serde_json::Value>, ApiClientError> {
        let url = format!("{}{}", self.api_client_config.base_url, path);
        let urn = format!("MULTIPART POST:{}", url);
        log::debug!("{}....", urn);
        // 请求并获取响应
        let response = REQWEST_CLIENT
            .post(&url)
            .multipart(form)
            .header(USER_ID_HEADER_NAME, current_user_id)
            .send()
            .await
            .map_err(|e| RequestError(urn.clone(), e))?;
        log::debug!("{} response....", urn);
        // 检查状态码，如果不是成功状态码则转换为错误
        let status_code = response.status();
        if !status_code.is_success() {
            return Err(ResponseStatusError(url.clone(), status_code.to_string()));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| ResponseError(url.clone(), e))?;
        log::debug!("{} response body: {}", urn, response_text);

        // 将文本解析为JSON
        let result: Ro<serde_json::Value> =
            serde_json::from_str(&response_text).map_err(|e| JsonParseError(url, e))?;
        Ok(result)
    }
}
