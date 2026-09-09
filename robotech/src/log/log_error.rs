use crate::cfg::CfgError;
use crate::env::EnvError;
use thiserror::Error;
use tracing_appender::rolling::InitError;

/// # 日志模块错误
///
/// 日志初始化、配置加载、文件监听等操作失败时返回的错误类型。
#[derive(Error, Debug)]
pub enum LogError {
    #[error("{0}")]
    Cfg(#[from] CfgError),
    #[error("{0}")]
    GetEnv(#[from] EnvError),
    #[error("Fail to watch file: {0}")]
    WatchFile(#[from] notify::Error),
    #[error("Fail to create file appender: {0}")]
    CreateFileAppender(InitError),
    #[error("Fail to set LOG_GUARD")]
    SetLogGuard(),
    #[error("Fail to set LOG_CONFIG_GUARD")]
    SetLogConfigGuard(),
}
