use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use wheel_rs::serde::path_buf_option_serde;

/// # HTTPS 配置
///
/// 用于配置是否启用 HTTPS，以及证书与私钥文件的路径。
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct HttpsConfig {
    /// 是否启用(默认: true)
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    /// 证书文件路径
    #[serde(with = "path_buf_option_serde")]
    pub cert: Option<PathBuf>,
    /// 密钥文件路径
    #[serde(with = "path_buf_option_serde")]
    pub key: Option<PathBuf>,
}

impl Default for HttpsConfig {
    fn default() -> Self {
        HttpsConfig {
            enabled: enabled_default(),
            cert: None,
            key: None,
        }
    }
}
fn enabled_default() -> bool {
    true
}
