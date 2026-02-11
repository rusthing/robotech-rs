use crate::cfg::CfgError;
use crate::env::EnvError;
use thiserror::Error;
use tracing_appender::rolling::InitError;

#[derive(Error, Debug)]
pub enum LogError {
    #[error("{0}")]
    Cfg(#[from] CfgError),
    #[error("{0}")]
    GetEnv(#[from] EnvError),
    #[error("Fail to create file appender: {0}")]
    CreateFileAppender(InitError),
    #[error("Fail to set LOG_GUARD")]
    SetLogGuard(),
}
