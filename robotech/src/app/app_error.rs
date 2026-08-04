use crate::cfg::CfgError;
use crate::env::EnvError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Get APP_CONFIG error")]
    GetAppConfig(),
    #[error("Set APP_CONFIG error")]
    SetAppConfig(),
    #[error("{0}")]
    GetEnv(#[from] EnvError),
    #[error("Config error: {0}")]
    Cfg(#[from] CfgError),
}
