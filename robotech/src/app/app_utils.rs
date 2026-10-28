use crate::app::AppError;
use crate::cfg::{build_cfg, deserialize_config, BaseConfig, CfgError};
use crate::env::{AppEnv, EnvError, APP_ENV};
use crate::log::LogConfig;
#[cfg(feature = "config-center")]
use crate::micro_svc::watch_config_changed;
use arc_swap::ArcSwap;
use config::{Config, Value};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tracing::{debug, error, info, warn};
use wheel_rs::config_utils::diff_config;
use wheel_rs::file_utils::{watch_file_changed, FileWatcher};
use wheel_rs::process::{get_current_pid, send_signal_by_instruction};

/// 应用层操作结果类型
pub type Result<T> = core::result::Result<T, AppError>;

/// # 应用配置监听器
///
/// 加载应用配置文件并监听其变化，在配置变更时自动重载，并通过回调通知调用方。
///
/// 监听期间会同时监听应用自身文件的更新，一旦应用文件被更新则触发优雅退出。
pub struct AppWatcher<T>
where
    T: Clone + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    _cfg_file_watcher: FileWatcher,
    _app_file_watcher: FileWatcher,
    pub app_config: Arc<T>,
    watch_join_handle: tokio::task::JoinHandle<()>,
}

impl<T> Drop for AppWatcher<T>
where
    T: Clone + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    /// 退出时 abort 后台配置监听任务
    fn drop(&mut self) {
        self.watch_join_handle.abort();
    }
}

impl<T> AppWatcher<T>
where
    T: Clone + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    /// # 创建应用配置监听器
    ///
    /// 加载应用配置文件（可通过 `config_file_path` 指定，未指定时使用环境中的
    /// 应用配置文件），解析出 `T` 类型的应用配置，并启动配置热更新监听。
    ///
    /// 当应用配置变化时会调用 `on_change` 回调；日志配置变化时通过
    /// `log_config_changed_tx` 发送新的日志配置。
    ///
    /// ## 参数
    /// * `config_file_path` - 可选的自定义配置文件路径
    /// * `log_config_changed_tx` - 日志配置变更通知发送端
    /// * `on_change` - 应用配置变更时的回调函数
    ///
    /// ## 返回值
    /// 返回 `Ok(AppWatcher<T>)`，持有最新的应用配置与后台监听任务；
    /// 初始化失败时返回 `Err(AppError)`。
    pub async fn new<F, Fut>(
        config_file_path: Option<String>,
        log_config_changed_tx: watch::Sender<(LogConfig, HashMap<String, Value>)>,
        mut on_change: F,
    ) -> Result<Self>
    where
        F: FnMut(Arc<T>, HashMap<String, Value>) -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let AppEnv {
            app_dir,
            app_file_path,
            app_file_name_without_ext,
            ..
        } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;
        let (config, files) =
            build_app_cfg(config_file_path.clone(), app_dir, app_file_name_without_ext).await?;
        let base_config: BaseConfig = config
            .clone()
            .try_deserialize()
            .map_err(CfgError::Deserialize)?;
        if let Some(log_config) = base_config.clone().log {
            let changed = HashMap::new();
            log_config_changed_tx.send((log_config, changed))?;
        }
        let app_config: T = deserialize_config::<T>(config.clone()).await?;

        let last_config = Arc::new(ArcSwap::from_pointee(config.clone()));
        let (config_changed_tx, mut config_changed_rx) =
            watch::channel((app_config.clone(), HashMap::new()));
        // let app_config_clone = app_config.clone();
        let watch_join_handle = tokio::spawn(async move {
            info!("watching app config...");
            loop {
                match config_changed_rx.changed().await {
                    Ok(_) => {
                        let (app_config, changed) = config_changed_rx.borrow().clone();
                        if let Err(e) = on_change(Arc::new(app_config), changed).await {
                            error!("handle config change error: {e:?}");
                            break;
                        }
                    }
                    Err(err) => {
                        info!("watch config error: {:?}", err);
                        break;
                    }
                }
            }
            info!("exit watch app config");
        });

        // 公共的配置重载逻辑
        type ReloadFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
        let reload_config_fn: Arc<dyn Fn() -> ReloadFuture + Send + Sync> = {
            let config_changed_tx = config_changed_tx.clone();
            let last_config = Arc::clone(&last_config);
            let config_file_path = config_file_path.clone();
            let app_dir = app_dir.clone();
            Arc::new(move || {
                let config_changed_tx = config_changed_tx.clone();
                let log_config_changed_tx_clone = log_config_changed_tx.clone();
                let last_config = Arc::clone(&last_config);
                let config_file_path = config_file_path.clone();
                let app_dir = app_dir.clone();
                let app_name = app_file_name_without_ext.clone();
                Box::pin(async move {
                    let old_config = last_config.load_full();
                    match build_app_cfg(config_file_path, &app_dir, &app_name).await {
                        Ok((new_config, _)) => {
                            let changed = diff_config(&old_config, &new_config);
                            if !changed.is_empty() {
                                info!("app config changed: {:?}", changed);
                                last_config.store(Arc::new(new_config.clone()));

                                let base_config: BaseConfig =
                                    match new_config.clone().try_deserialize() {
                                        Ok(base_config) => base_config,
                                        Err(e) => {
                                            error!("deserialize base config error: {:?}", e);
                                            return;
                                        }
                                    };
                                if let Some(log_config) = base_config.clone().log {
                                    let changed = HashMap::new();
                                    if let Err(e) =
                                        log_config_changed_tx_clone.send((log_config, changed))
                                    {
                                        error!("send log config changed error: {:?}", e);
                                    }
                                }

                                match deserialize_config::<T>(new_config).await {
                                    Ok(app_config) => {
                                        if let Err(e) =
                                            config_changed_tx.send((app_config, changed))
                                        {
                                            error!("send config changed error: {:?}", e);
                                        }
                                    }
                                    Err(e) => error!("deserialize app config error: {:?}", e),
                                }
                            }
                        }
                        Err(e) => error!("build app config error: {:?}", e),
                    }
                })
            })
        };

        // 文件变更回调
        let reload = Arc::clone(&reload_config_fn);
        let _cfg_file_watcher =
            watch_file_changed(files.clone(), base_config.watch_debounce_delay, move |_| {
                let reload = Arc::clone(&reload);
                async move {
                    reload().await;
                    #[cfg(feature = "config-center")]
                    register_config_center_watch(Arc::clone(&reload)).await;
                    Ok(())
                }
            })?;

        // 监听应用程序的文件变化，当文件更新时优雅退出应用程序
        let _app_file_watcher =
            watch_app_file(&app_file_path.clone(), base_config.watch_debounce_delay)?;

        #[cfg(feature = "config-center")]
        {
            register_config_center_watch(Arc::clone(&reload_config_fn)).await;
            spawn_config_fallback(Arc::clone(&reload_config_fn));
        }

        Ok(Self {
            app_config: Arc::new(app_config.clone()),
            watch_join_handle,
            _cfg_file_watcher,
            _app_file_watcher,
        })
    }
}

/// 构建应用配置对象。
///
/// 加载应用配置文件，并从环境变量（前缀 `APP`）中覆盖配置项。
async fn build_app_cfg(
    config_file_path: Option<String>,
    app_dir: &PathBuf,
    app_file_name_without_ext: &str,
) -> crate::cfg::Result<(Config, Vec<String>)> {
    let (config, files) = build_cfg(
        app_dir,
        "APP",
        Some(app_file_name_without_ext),
        app_file_name_without_ext,
        config_file_path,
    )
    .await?;

    // 刷新信号配置随应用配置一并加载：首次启动与热更新都经过这里，
    // 因此 `[refresh] probe-interval` 的修改无需重启即可在下一轮探活生效
    #[cfg(feature = "refresh")]
    crate::refresh::load_config(&config);

    Ok((config, files))
}

/// 注册配置中心变更监听。HubClient 重建后需要重新调用此函数。
#[cfg(feature = "config-center")]
fn register_config_center_watch(
    reload: Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async move {
        let reload_clone = Arc::clone(&reload);
        if let Err(e) = watch_config_changed(move || {
            let reload = Arc::clone(&reload_clone);
            async move {
                // 用 tokio::spawn 避免旧 HubClient drop 时
                // abort 掉当前 watch task 导致回调被取消
                tokio::spawn(async move {
                    reload().await;
                    register_config_center_watch(reload).await;
                });
                Ok(())
            }
        })
        .await
        {
            warn!("watch config center failed: {:?}", e);
        }
    })
}

/// 配置兜底刷新间隔
///
/// watch 链路本身已有自愈（etcd 断线重建、consul 长轮询重试），但仍有极小
/// 窗口可能漏事件：reload 读取配置之后、新 watch 挂上之前发生的变更。
/// 这里定期做一次全量拉取 + diff——没有变化时 `reload` 内部不会触发 `setup()`，
/// 代价只是一次配置中心读取。
#[cfg(feature = "config-center")]
const CONFIG_FALLBACK_INTERVAL: Duration = Duration::from_secs(600);

/// 启动配置兜底刷新任务。
#[cfg(feature = "config-center")]
fn spawn_config_fallback(
    reload: Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(CONFIG_FALLBACK_INTERVAL).await;
            reload().await;
        }
    });
}

/// # 监听应用程序文件变化并触发优雅退出
///
/// 当应用程序自身的可执行文件被更新时，通过发送 `quit` 信号触发优雅退出。
///
/// ## 参数
/// * `app_file_path` - 应用程序文件路径
/// * `debounce_delay` - 文件变更去抖动延迟
///
/// ## 返回值
/// 返回 `Ok(FileWatcher)`；监听创建失败时返回 `Err(AppError)`
pub fn watch_app_file(app_file_path: &Path, debounce_delay: Duration) -> Result<FileWatcher> {
    let files = vec![app_file_path.to_string_lossy().to_string()];
    Ok(watch_file_changed(files, debounce_delay, |_| async {
        info!("应用程序的文件已更新，优雅退出");
        quit();
        Ok(())
    })?)
}

/// # 等待退出信号并执行优雅退出
///
/// 循环监听 `signal_receiver` 中的系统信号，收到 SIGINT / SIGTERM / SIGQUIT 时
/// 停止监听，随后执行 `graceful_shutdown` 回调完成优雅退出。
///
/// ## 参数
/// * `signal_receiver` - 系统信号广播接收端
/// * `graceful_shutdown` - 优雅退出时执行的收尾逻辑
///
/// ## 返回值
/// 优雅退出流程执行完成后返回 `Ok(())`；收尾逻辑失败时返回 `Err(AppError)`。
pub async fn wait_app_exit<F, Fut>(
    mut signal_receiver: broadcast::Receiver<nix::sys::signal::Signal>,
    graceful_shutdown: F,
) -> Result<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    loop {
        match signal_receiver.recv().await {
            Ok(signal) => {
                debug!("收到信号: {:?}", signal);
                match signal {
                    nix::sys::signal::Signal::SIGINT
                    | nix::sys::signal::Signal::SIGTERM
                    | nix::sys::signal::Signal::SIGQUIT => {
                        break;
                    }
                    _ => {}
                }
            }
            Err(err) => {
                warn!("无法接收信号: {}", err);
                break;
            }
        }
    }
    debug!("正在优雅退出...");
    graceful_shutdown().await?;
    debug!("优雅退出完成.");
    Ok(())
}

/// 向当前进程发送 `quit` 信号，触发优雅退出
/// 注意：此函数为同步函数，在异步上下文中调用时不会阻塞
fn quit() {
    let _ = send_signal_by_instruction("quit", get_current_pid());
}

/// # 初始化应用的可选子系统
///
/// 由 `bootstrap!` 生成的 `main()` 在启动早期调用。**按 robotech 自身启用的 feature
/// 决定实际初始化内容**：
/// - 启用 `db` 时初始化数据访问层的唯一键 / 外键注册表
///
/// 未启用相应 feature 时为空操作。因此，**不需要数据库 / Web 的服务也可以使用
/// `bootstrap!`，无需为其引入 `db` / `web` 相关依赖**。
///
/// ## 返回值
/// 初始化成功返回 `Ok(())`；失败时返回 `Err(AppError)`
pub fn init_app() -> Result<()> {
    #[cfg(feature = "db")]
    crate::dao::init_dao()?;
    Ok(())
}

/// # 注册当前服务实例
///
/// 由 `bootstrap!` 生成的 `main()` 在 `setup` 执行完成后调用。
/// 启用 `config-center` / `registry-center` 时把当前服务注册到注册中心；
/// 未启用时为空操作。
pub async fn register_app() {
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    crate::micro_svc::register_micro_svc().await;
    // 自动分配的 worker node 可能存在"同时启动选中同一个号"的窗口，
    // 首次注册后做一次冲突检查（未启用自动分配时为空操作）
    crate::app::spawn_id_node_conflict_check();
}

/// # 优雅退出时清理应用的可选子系统
///
/// 由 `bootstrap!` 生成的 `main()` 在收到退出信号后调用。**按 robotech 自身启用的
/// feature 决定实际清理内容**：
/// - 先执行通过 [`register_shutdown_hook`] 注册的停机钩子（见下）
/// - 启用 `config-center` / `registry-center` 时注销微服务实例
/// - 启用 `web` 时停止 Web 服务
///
/// 未启用相应 feature 时为空操作。
///
/// ## 为什么钩子排在最前面
///
/// 钩子往往要发 HTTP 请求（例如 jobx 执行器把「已领取但还没跑」的任务上报为失败）。
/// 注销注册中心、停 Web 服务都会耗时甚至阻塞，排在它们后面可能还没轮到进程就退出了。
/// 钩子本身另有 [`SHUTDOWN_HOOK_TIMEOUT`] 兜底，不会把退出流程拖死。
pub async fn shutdown_app() {
    run_shutdown_hooks().await;
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    crate::micro_svc::drop_hub_client().await;
    #[cfg(feature = "web")]
    if let Err(e) = crate::web::stop_web_service().await {
        error!("无法停止旧的Web服务: {e:?}");
    }
}

/// 单个停机钩子的最长执行时间。
///
/// 钩子跑在退出流程的关键路径上，必须有上限：卡住的钩子不能让进程等到被 SIGKILL。
/// 10s 足够一次本地/内网 HTTP 往返，也远小于 k8s 默认的 30s `terminationGracePeriodSeconds`。
pub const SHUTDOWN_HOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// 停机钩子：调用后返回一个待 await 的 future。
///
/// 用 `FnOnce` 而不是 `Fn`：钩子只在退出时执行一次，允许消耗自己的捕获变量
/// （典型场景是把 `Arc<...>`、连接池句柄 move 进去）。
type ShutdownHook = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// 停机钩子注册表。
///
/// 用 `OnceLock<Mutex<Vec<_>>>` 而不是 `static mut` / `lazy_static`：
/// 注册发生在各子系统的 `setup_*` 阶段（可能是多线程 runtime 里的任意线程），
/// 需要一个进程级、线程安全且可重复写入的容器。
static SHUTDOWN_HOOKS: OnceLock<Mutex<Vec<ShutdownHook>>> = OnceLock::new();

fn shutdown_hooks() -> &'static Mutex<Vec<ShutdownHook>> {
    SHUTDOWN_HOOKS.get_or_init(|| Mutex::new(Vec::new()))
}

/// # 注册一个停机钩子
///
/// 钩子会在进程收到 SIGINT / SIGTERM / SIGQUIT、`bootstrap!` 生成的 `main()` 调用
/// [`shutdown_app`] 时被**依次执行一次**，执行顺序即注册顺序。
///
/// ## 适用场景
///
/// 子系统有「进程退出后无法自动恢复」的在途状态，必须在退出前主动收尾。典型例子是
/// jobx 执行器：它领取任务后会先在数据库里落一条「运行中」的记录，进程若直接退出，
/// 这条记录就永远停在「运行中」（服务端只能靠超时巡检兜底）。注册一个钩子把这类任务
/// 主动上报为失败，能让状态在秒级收敛，而不是等巡检。
///
/// ## 约束
///
/// - 钩子**不能**依赖 `main` 之后才存在的状态，也不应无限阻塞：单个钩子超过
///   [`SHUTDOWN_HOOK_TIMEOUT`] 会被跳过并打 `warn`。
/// - 钩子可以调用异步 API（HTTP、DB），但退出流程此时可能正在拆解其它子系统，
///   能拿到什么就只用什么，失败要自己吞掉，不要 `panic`。
/// - 同一个钩子重复注册会被执行多次，需要幂等的场景请自行用 `OnceLock` 保护。
pub fn register_shutdown_hook<F, Fut>(hook: F)
where
    F: FnOnce() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let hook: ShutdownHook = Box::new(move || Box::pin(hook()));
    let mut hooks = shutdown_hooks()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    hooks.push(hook);
}

/// # 执行并清空所有停机钩子
///
/// 由 [`shutdown_app`] 调用。取出全部钩子后**先清空注册表再逐个执行**，
/// 这样即使某个钩子内部又触发了一次退出流程，也不会重复执行同一批钩子。
pub async fn run_shutdown_hooks() {
    let hooks: Vec<ShutdownHook> = {
        let mut guard = shutdown_hooks()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *guard)
    };
    if hooks.is_empty() {
        return;
    }
    let total = hooks.len();
    info!("执行 {total} 个停机钩子...");
    for (index, hook) in hooks.into_iter().enumerate() {
        match tokio::time::timeout(SHUTDOWN_HOOK_TIMEOUT, hook()).await {
            Ok(()) => debug!("停机钩子 #{}/{total} 完成", index + 1),
            Err(_) => warn!(
                "停机钩子 #{}/{total} 超时（>{SHUTDOWN_HOOK_TIMEOUT:?}），跳过",
                index + 1
            ),
        }
    }
    info!("停机钩子执行完毕");
}
