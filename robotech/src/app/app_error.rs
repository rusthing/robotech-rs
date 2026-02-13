use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Get APP_CONFIG error")]
    GetAppConfig(),
    #[error("Set APP_CONFIG error")]
    SetAppConfig(),
}
