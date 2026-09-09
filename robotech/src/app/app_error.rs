use crate::cfg::CfgError;
use crate::env::EnvError;
use crate::log::LogConfig;
#[cfg(feature = "registry-center")]
use crate::micro_svc::RegistryCenterError;
use config::Value;
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::watch;

/// # 应用层错误
///
/// 应用配置获取/设置、环境变量读取、配置解析、文件监听等操作失败时返回的错误类型。
#[derive(Error, Debug)]
pub enum AppError {
    /// 获取全局应用配置失败
    #[error("Get APP_CONFIG error")]
    GetAppConfig(),
    /// 设置全局应用配置失败
    #[error("Set APP_CONFIG error")]
    SetAppConfig(),
    /// 读取环境信息失败
    #[error("{0}")]
    GetEnv(#[from] EnvError),
    /// 配置解析失败
    #[error("Config error: {0}")]
    Cfg(#[from] CfgError),
    /// 监听文件失败
    #[error("{0}")]
    WatchFile(#[from] notify::Error),
    /// 配置变更通知发送失败
    #[error("watch sender send error: {0}")]
    WatchSend(#[from] watch::error::SendError<(LogConfig, HashMap<String, Value>)>),
    /// 注册中心操作失败（feature = "registry-center"）
    #[cfg(feature = "registry-center")]
    #[error("Registry center error: {0}")]
    Registry(#[from] RegistryCenterError),
}