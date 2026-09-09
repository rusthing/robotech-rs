use crate::env::EnvError;
use std::path::PathBuf;
use thiserror::Error;
use wheel_rs::process::{PidError, ProcessError};

/// # 信号管理错误
///
/// 信号管理相关的错误类型，涵盖环境信息获取、PID 文件读写、进程操作等失败场景。
#[derive(Error, Debug)]
pub enum SignalManagerError {
    #[error("{0}")]
    GetEnv(#[from] EnvError),
    #[error("PID error: {0}")]
    Pid(#[from] PidError),
    #[error("Process error: {0}")]
    Process(#[from] ProcessError),
    #[error("PID file not found: {0}")]
    NotFoundPidFile(PathBuf),
    #[error("Program is running: {0}")]
    ProgramIsRunning(u32),
}
