use crate::env::EnvError;
use arc_swap::ArcSwapOption;
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use ulid::Ulid;

/// # 全局应用环境
///
/// 存储初始化后的应用环境信息（见 [`AppEnv`]），由 [`init_env`] 完成初始化，
/// 仅允许设置一次。
pub static APP_ENV: OnceLock<AppEnv> = OnceLock::new();

/// # 应用名称（全局可覆盖）
///
/// 初始值为可执行文件名（不含扩展名），后续可通过配置中心的 `app_name` 覆盖。
/// 使用 [`get_app_name`] 读取，[`set_app_name`] 设置。
static APP_NAME: ArcSwapOption<String> = ArcSwapOption::const_empty();

/// 获取当前应用名称。
///
/// 返回初始值（可执行文件名）或通过 [`set_app_name`] 覆盖后的值。
pub fn get_app_name() -> String {
    APP_NAME.load_full().as_deref().cloned().unwrap_or_default()
}

/// 设置应用名称（通常在加载配置后，用配置中的 `app_name` 覆盖初始值）。
pub fn set_app_name(name: String) {
    APP_NAME.store(Some(Arc::new(name)));
}

/// # 应用实例 ID（全局可覆盖）
///
/// 在 [`init_env`] 中初始化为 ULID；Web 服务器启动后可通过 [`set_app_instance_id`]
/// 更新为 `{app_name}-{ip}-{port}` 格式。
static APP_INSTANCE_ID: ArcSwapOption<String> = ArcSwapOption::const_empty();

/// 获取当前应用实例 ID。
///
/// 初始值为 ULID；若 Web 服务器已启动且调用了 [`set_app_instance_id`]，
/// 则返回更新后的 `{app_name}-{ip}-{port}` 值。
pub fn get_app_instance_id() -> String {
    APP_INSTANCE_ID.load_full().as_deref().cloned().unwrap_or_default()
}

/// 设置应用实例 ID（通常在 Web 服务器启动、获取端口后调用）。
pub fn set_app_instance_id(id: String) {
    APP_INSTANCE_ID.store(Some(Arc::new(id)));
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
    let app_file_path_without_ext = app_file_path.clone().with_extension("");

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

    // 生成 ULID（时间可排序的唯一标识）
    let ulid = Ulid::new().to_string();

    // 初始化全局应用名称（后续可通过配置覆盖）
    set_app_name(app_file_name_without_ext.clone());

    // 初始化全局应用实例 ID（后续 Web 服务器启动后可覆盖为含端口格式）
    set_app_instance_id(ulid);

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