use crate::micro_svc::config_center::ConfigKey;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigCenterError {
    #[error("配置未找到: {0}")]
    NotFound(ConfigKey),

    #[error("无法识别的文件格式: {0}")]
    UnknownFileFormat(String),

    #[error("配置内容解析失败: {0}")]
    Parse(String),

    #[error("后端连接/请求失败: {0}")]
    Connection(String),

    #[error("本地快照读写失败: {0}")]
    Cache(String),

    #[error("不支持的后端: {0}（对应 Cargo feature 未启用，检查 Cargo.toml 里的 features）")]
    BackendNotEnabled(String),

    #[error("配置错误: {0}")]
    Internal(String),
}
