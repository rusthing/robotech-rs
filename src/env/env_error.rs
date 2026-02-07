use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EnvError {
    #[error("Failed to get application path: {0}")]
    GetAppPath(io::Error),
    #[error("Failed to get application file name")]
    GetAppFileName(),
    #[error("Failed to set environment variable")]
    SetEnv(),
    #[error("Failed to get environment variable")]
    GetEnv(),
}
