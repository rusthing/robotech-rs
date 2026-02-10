use crate::env::EnvError;
use config::ConfigError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppConfigError {
    #[error("{0}")]
    GetEnv(#[from] EnvError),
    #[error("Fail to build: {0}")]
    Build(ConfigError),
    #[error("Fail to deserialize: {0}")]
    Deserialize(ConfigError),
}
