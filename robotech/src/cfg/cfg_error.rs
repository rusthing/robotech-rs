use crate::env::EnvError;
use config::ConfigError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CfgError {
    #[error("{0}")]
    GetEnv(#[from] EnvError),
    #[error("Fail to build config: {0}")]
    Build(ConfigError),
    #[error("Fail to deserialize config: {0}")]
    Deserialize(ConfigError),
}
