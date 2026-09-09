use std::io;
use thiserror::Error;

/// # 环境错误枚举
///
/// 该枚举定义了应用环境初始化过程中可能出现的错误类型，
/// 包括获取应用路径、获取应用文件名以及设置/获取 `APP_ENV` 失败等。
#[derive(Error, Debug)]
pub enum EnvError {
    #[error("Failed to get application path: {0}")]
    GetAppPath(io::Error),
    #[error("Failed to get application file name")]
    GetAppFileName(),
    #[error("Failed to set APP_ENV")]
    SetAppEnv(),
    #[error("Failed to get APP_ENV")]
    GetAppEnv(),
}
