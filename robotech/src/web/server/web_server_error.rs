use crate::env::EnvError;
use std::io;
use thiserror::Error;
use wheel_rs::process::ProcessError;

/// # Web 服务器错误
///
/// 该枚举定义了 Web 服务器在配置解析、端口解析、CORS/HTTPS 配置、监听绑定、
/// 启动超时、服务停止与旧应用终止等过程中可能出现的错误。
#[derive(Error, Debug)]
pub enum WebServerError {
    #[error("{0}")]
    GetEnv(#[from] EnvError),
    #[error("Fail to parse config: {0}")]
    Config(String),
    #[error("Fail to parse port: {0}")]
    ParsePort(String),
    #[error("Fail to parse listen binds: {0}")]
    ParseListenBinds(String),
    #[error("Fail to parse CORS config form {0}: {1}")]
    ParseCors(String, String),
    #[error("Fail to parse HTTPS cert: {0}")]
    ParseHttpsCert(String),
    #[error("Fail to parse HTTPS key: {0}")]
    ParseHttpsKey(String),
    #[error("Start web server timeout: {0}")]
    StartWebServerTimeout(String),
    #[error("Fail to stop service: {0}")]
    StopService(String),
    #[error("Fail to terminate old app: {0}")]
    TerminateOldApp(#[from] ProcessError),
    #[error("Socket error: {0}")]
    Socket(String),
    #[error("Web server runtime error: {0}")]
    Runtime(#[from] io::Error),
    #[error("Fail to set web server handles: {0}")]
    SetWebServiceHandles(String),
    #[error("Fail to take web server handles: {0}")]
    TakeWebServiceHandles(String),
    #[error("Fail to build reqwest client: {0}")]
    BuildReqwestClient(String),
}
