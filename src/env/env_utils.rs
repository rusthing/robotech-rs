use crate::env::EnvError;
use log::info;
use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;

/// 全局配置
pub static APP_ENV: OnceLock<AppEnv> = OnceLock::new();

#[derive(Debug)]
pub struct AppEnv {
    pub app_file_path: PathBuf,
    pub app_file_path_without_ext: PathBuf,
    pub app_dir: PathBuf,
    pub app_file_name: String,
    pub app_file_name_without_ext: String,
}

/// 初始化环境变量
pub fn init_env() -> Result<(), EnvError> {
    info!("init env...");
    // 获取当前执行文件路径
    let app_file_path = env::current_exe().map_err(EnvError::GetAppPath)?;

    // 获取当前执行文件路径(不带后缀)
    let mut app_file_path_without_ext = app_file_path.clone();
    app_file_path_without_ext.pop();

    // 获取当前执行文件所在目录
    let mut app_dir = app_file_path.clone();
    app_dir.pop();

    // 获取当前执行文件名
    let app_file_name = app_file_path
        .file_name()
        .ok_or(EnvError::GetAppFileName())?
        .to_string_lossy()
        .to_string();

    // 获取当前执行文件名(不带后缀)
    let app_file_name_without_ext = app_file_path
        .file_stem()
        .ok_or(EnvError::GetAppFileName())?
        .to_string_lossy()
        .to_string();

    let env = AppEnv {
        app_file_path,
        app_file_path_without_ext,
        app_dir,
        app_file_name,
        app_file_name_without_ext,
    };

    APP_ENV.set(env).map_err(|_| EnvError::SetAppEnv())?;
    Ok(())
}
