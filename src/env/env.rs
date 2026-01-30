use log::info;
use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;

/// 全局配置
pub static ENV: OnceLock<Env> = OnceLock::new();

#[derive(Debug)]
pub struct Env {
    pub app_file_path: PathBuf,
    pub app_dir: PathBuf,
    pub app_file_name: String,
}

/// 初始化环境变量
pub fn init_env() {
    info!("init env...");
    let app_file_path = env::current_exe().expect("Failed to get application path");
    let app_file_name = app_file_path
        .file_name()
        .expect("Failed to get application file name")
        .to_string_lossy()
        .to_string();
    // 获取当前执行文件所在目录
    let mut app_dir = app_file_path.clone();
    app_dir.pop();

    let env = Env {
        app_file_path,
        app_dir,
        app_file_name,
    };

    ENV.set(env).expect("Unable to set environment variables");
}
