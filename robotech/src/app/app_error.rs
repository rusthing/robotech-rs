use crate::cfg::CfgError;
use crate::env::EnvError;
use crate::log::LogConfig;
#[cfg(feature = "registry-center")]
use crate::micro_svc::RegistryCenterError;
use config::Value;
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::watch;

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
    #[error("{0}")]
    WatchFile(#[from] notify::Error),
    #[error("watch sender send error: {0}")]
    WatchSend(#[from] watch::error::SendError<(LogConfig, HashMap<String, Value>)>),
    #[cfg(feature = "registry-center")]
    #[error("Registry center error: {0}")]
    Registry(#[from] RegistryCenterError),
}