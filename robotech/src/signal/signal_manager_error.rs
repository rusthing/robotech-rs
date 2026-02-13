use crate::env::EnvError;
use libc::pid_t;
use std::path::PathBuf;
use thiserror::Error;
use wheel_rs::process::{PidError, ProcessError};

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
    ProgramIsRunning(pid_t),
}
