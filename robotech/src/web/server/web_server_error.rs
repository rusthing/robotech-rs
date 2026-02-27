use std::io;
use thiserror::Error;
use wheel_rs::process::ProcessError;

#[derive(Error, Debug)]
pub enum WebServerError {
    #[error("Fail to parse port: {0}")]
    ParsePort(String),
    #[error("Fail to parse listen binds: {0}")]
    ParseListenBinds(String),
    #[error("Fail to parse CORS config form {0}: {1}")]
    ParseCors(String, String),
    #[error("Start web server timeout: {0}")]
    StartWebServerTimeout(String),
    #[error("Fail to terminate old web server: {0}")]
    TerminateOldWebServer(#[from] ProcessError),
    #[error("Socket error: {0}")]
    Socket(String),
    #[error("Web server runtime error: {0}")]
    Runtime(#[from] io::Error),
    #[error("Fail to set web server handles")]
    SetWebServerHandles(),
    #[error("Fail to get web server handles")]
    GetWebServerHandles(),
}
