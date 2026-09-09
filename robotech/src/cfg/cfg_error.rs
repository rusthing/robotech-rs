use crate::env::EnvError;
use config::ConfigError;
use thiserror::Error;

/// # 配置错误枚举
///
/// 该枚举定义了配置加载、反序列化与初始化过程中可能出现的错误类型，
/// 包括获取环境错误、构建/反序列化失败以及初始化状态错误等。
#[derive(Error, Debug)]
pub enum CfgError {
    #[error("{0}")]
    GetEnv(#[from] EnvError),
    #[error("Fail to build config: {0}")]
    Build(ConfigError),
    #[error("Fail to deserialize config: {0}")]
    Deserialize(ConfigError),
    #[error("Fail to init config: {0}")]
    Init(String),
    #[error("Config not initialized: {0}")]
    NotInit(String),
}
