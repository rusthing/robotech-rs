use crate::env::EnvError;
use arc_swap::ArcSwap;
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, OnceLock};
use uuid::Uuid;

/// # 全局应用环境
///
/// 存储初始化后的应用环境信息（见 [`AppEnv`]），由 [`init_env`] 完成初始化，
/// 仅允许设置一次。
pub static APP_ENV: OnceLock<AppEnv> = OnceLock::new();

/// # 应用名称（全局可覆盖）
///
/// 初始值为可执行文件名（不含扩展名），后续可通过配置中心的 `app_name` 覆盖。
/// 使用 [`get_app_name`] 读取，[`set_app_name`] 设置。
static APP_NAME: LazyLock<ArcSwap<String>> =
    LazyLock::new(|| ArcSwap::from(Arc::new(String::new())));

/// 获取当前应用名称
pub fn get_app_name() -> String {
    APP_NAME.load_full().as_ref().clone()
}

/// 设置应用名称（通常在加载配置后，用配置中的 app_name 覆盖初始值）
pub fn set_app_name(name: String) {
    APP_NAME.store(Arc::new(name));
}

/// # 应用环境信息
///
/// 描述当前执行文件相关的路径与名称信息，由 [`init_env`] 初始化。
#[derive(Debug)]
pub struct AppEnv {
    /// 当前执行文件的完整路径
    pub app_file_path: PathBuf,
    /// 当前执行文件路径（不含扩展名）
    pub app_file_path_without_ext: PathBuf,
    /// 当前执行文件所在目录
    pub app_dir: PathBuf,
    /// 当前执行文件名（含扩展名）
    pub app_file_name: String,
    /// 当前执行文件名（不含扩展名）
    pub app_file_name_without_ext: String,
    /// 当前实例ID
    pub instance_id: String,
}

/// # 初始化应用环境
///
/// 获取当前执行文件的路径、目录与名称信息，并写入全局环境 `APP_ENV`。
///
/// ## 返回值
/// 初始化成功返回 `Ok(())`
///
/// ## 错误
/// 获取执行文件路径失败时返回 `EnvError::GetAppPath`；
/// 获取执行文件名失败时返回 `EnvError::GetAppFileName`；
/// 环境已初始化导致写入失败时返回 `EnvError::SetAppEnv`
pub fn init_env() -> Result<(), EnvError> {
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

    // 初始化全局应用名称（后续可通过配置覆盖）
    let app_file_name_without_ext_clone = app_file_name_without_ext.clone();

    let env = AppEnv {
        app_file_path,
        app_file_path_without_ext,
        app_dir,
        app_file_name,
        app_file_name_without_ext,
        instance_id: Uuid::new_v4().to_string(),
    };

    APP_ENV.set(env).map_err(|_| EnvError::SetAppEnv())?;

    set_app_name(app_file_name_without_ext_clone);

    Ok(())
}