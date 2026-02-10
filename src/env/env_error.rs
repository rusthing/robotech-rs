use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EnvError {
    #[error("Failed to get application path: {0}")]
    GetAppPath(io::Error),
    #[error("Failed to get application file name")]
    GetAppFileName(),
    #[error("Failed to set ENV")]
    SetEnv(),
    #[error("Failed to get ENV")]
    GetEnv(),
    #[error("Invalid environment variable: {0}-{1}, only support {2}")]
    InvalidEnvironmentVariable(String, String, String),
}
