use std::io;
use thiserror::Error;
use wheel_rs::process::ProcessError;

#[derive(Error, Debug)]
pub enum WebServerError {
    #[error("Fail to parse port: {0}")]
    ParsePort(String),
    #[error("Start web server timeout: {0}")]
    StartWebServerTimeout(String),
    #[error("Fail to terminate old web server: {0}")]
    TerminateOldWebServer(#[from] ProcessError),
    #[error("Socket error: {0}")]
    Socket(String),
    #[error("Web server runtime error: {0}")]
    Runtime(#[source] io::Error),
}
