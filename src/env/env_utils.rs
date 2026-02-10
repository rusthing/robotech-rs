use crate::env::EnvError;
use log::info;
use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;
use tracing_appender::rolling::Rotation;

/// 全局配置
pub static ENV: OnceLock<Env> = OnceLock::new();

static LOG_ROTATION_ENV_VAR: &str = "LOG_ROTATION";

#[derive(Debug)]
pub struct Env {
    pub app_file_path: PathBuf,
    pub app_dir: PathBuf,
    pub app_file_name: String,
    pub log_rotation: Rotation,
}

/// 初始化环境变量
pub fn init_env() -> Result<(), EnvError> {
    info!("init env...");
    let app_file_path = env::current_exe().map_err(EnvError::GetAppPath)?;
    let app_file_name = app_file_path
        .file_name()
        .ok_or(EnvError::GetAppFileName())?
        .to_string_lossy()
        .to_string();
    // 获取当前执行文件所在目录
    let mut app_dir = app_file_path.clone();
    app_dir.pop();

    let log_rotation = env::var(LOG_ROTATION_ENV_VAR).unwrap_or("hourly".to_string());
    let log_rotation = log_rotation.trim().to_lowercase();
    let log_rotation = match log_rotation.as_str() {
        "weekly" => Rotation::WEEKLY,
        "daily" => Rotation::DAILY,
        "hourly" => Rotation::HOURLY,
        "minutely" => Rotation::MINUTELY,
        "never" => Rotation::NEVER,
        _ => Err(EnvError::InvalidEnvironmentVariable(
            LOG_ROTATION_ENV_VAR.to_string(),
            log_rotation.to_string(),
            "weekly/daily/hourly/minutely/never".to_string(),
        ))?,
    };

    let env = Env {
        app_file_path,
        app_dir,
        app_file_name,
        log_rotation,
    };

    ENV.set(env).map_err(|_| EnvError::SetEnv())?;
    Ok(())
}
