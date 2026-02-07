use thiserror::Error;

#[derive(Error, Debug)]
pub enum LogError {
    #[error("Fail to create log directory: {0}")]
    CreateDirectory(String),
    #[error("Fail to set LOG_GUARD")]
    SetLogGuard(),
}
