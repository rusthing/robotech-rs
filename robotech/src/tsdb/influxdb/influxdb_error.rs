use reqwest::Error as ReqwestError;
use thiserror::Error;

/// InfluxDB 客户端构建/请求过程中的错误类型。
#[derive(Error, Debug)]
pub enum InfluxdbError {
    /// 构建底层 HTTP 客户端失败（如 reqwest 配置错误）
    #[error("构建错误: {0}")]
    Build(#[from] ReqwestError),
    /// 发送写入请求失败（网络、超时等）
    #[error("请求错误: {0}")]
    Request(String),
    /// InfluxDB 返回非 2xx 状态码
    #[error("InfluxDB 返回 {status}: {body}")]
    Api { status: u16, body: String },
}
