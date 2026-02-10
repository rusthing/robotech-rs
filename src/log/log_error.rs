use crate::env::EnvError;
use config::ConfigError;
use thiserror::Error;
use tracing_appender::rolling::InitError;

#[derive(Error, Debug)]
pub enum LogError {
    #[error("Fail to build: {0}")]
    BuildConfig(ConfigError),
    #[error("Fail to deserialize: {0}")]
    DeserializeConfig(ConfigError),
    #[error("{0}")]
    GetEnv(#[from] EnvError),
    #[error("Fail to create file appender: {0}")]
    CreateFileAppender(InitError),
    #[error("Fail to set LOG_GUARD")]
    SetLogGuard(),
}
