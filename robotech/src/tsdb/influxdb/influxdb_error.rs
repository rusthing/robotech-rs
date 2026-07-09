use reqwest::Error as ReqwestError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InfluxdbError {
    #[error("构建错误: {0}")]
    Build(#[from] ReqwestError),
}
