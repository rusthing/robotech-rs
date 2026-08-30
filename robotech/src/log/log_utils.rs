use crate::cfg::{build_cfg, CfgError};
use crate::cfg::{deserialize_config, BaseConfig};
use crate::env::{AppEnv, EnvError, APP_ENV};
use crate::log::{LogConfig, LogError};
use arc_swap::ArcSwap;
use config::{Config, Value};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::sync::watch;
use tracing::{debug, error, info};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::RollingFileAppender;
use tracing_core::{Event, Level, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::fmt::format::{DefaultFields, Writer};
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, reload, EnvFilter};
use wheel_rs::config_utils::diff_config;
use wheel_rs::file_utils::{watch_file_changed, FileWatcher};

/// 日志文件输出锁
/// 解决锁在初始化方法结束后被提前释放导致后续日志不能输出
static LOG_GUARD: RwLock<Option<WorkerGuard>> = RwLock::new(None);

struct CustomConsoleFormatter {
    /// 时间格式
    timer_format: String,
    /// 是否打印 span 链（包括函数名和参数，需 #[instrument] 配合）
    show_spans: bool,
}

impl CustomConsoleFormatter {
    pub fn new(timer_format: String, show_spans: bool) -> Self {
        Self {
            timer_format,
            show_spans,
        }
    }
}

impl<S, N> FormatEvent<S, N> for CustomConsoleFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let normalized_metadata = event.normalized_metadata();
        let metadata = normalized_metadata
            .as_ref()
            .unwrap_or_else(|| event.metadata());

        // 根据日志级别设置不同字体颜色
        let level = metadata.level();
        write!(
            writer,
            "\x1B[{}m ",
            match *level {
                Level::TRACE => 37,
                Level::DEBUG => 32,
                Level::INFO => 97,
                Level::WARN => 33,
                Level::ERROR => 31,
            }
        )?;

        let time_str = chrono::Local::now()
            .format(self.timer_format.as_str())
            .to_string();
        write!(writer, "{} ", time_str)?;

        write!(writer, "{:<5} ", *level)?;

        // 格式化事件字段
        // 设置字体颜色
        let visitor = DefaultFields::default();
        visitor.format_fields(writer.by_ref(), event)?;

        // 添加一个分隔符"-"
        write!(writer, " \x1B[1;93m-\x1B[0m ")?;

        // 输出 target（用于调试模块级别配置）
        write!(writer, "[{}] ", metadata.target())?;

        // 获取文件和行号信息
        // 设置字体颜色为蓝色
        write!(writer, "\x1B[34m")?;
        if let (Some(file_path), Some(line_number)) = (metadata.file(), metadata.line()) {
            let current_dir = env::current_dir().map_err(|_| std::fmt::Error)?;
            let absolute_path = current_dir.join(file_path);
            let path = format!("{}:{}", absolute_path.display(), line_number);
            let label = format!("{}:{}", file_path, line_number);
            write!(
                writer,
                "\x1B]8;;file://{}\x1B\\{}\x1B]8;;\x1B\\",
                path, label
            )?;
        }

        // 打印 span 链（包括函数名和参数）
        if self.show_spans {
            if let Some(scope) = _ctx.event_scope() {
                for span in scope.from_root() {
                    // 添加一个箭头"->"
                    write!(writer, " \x1B[1;93m->\x1B[0m ")?;
                    // 设置字体颜色为蓝色
                    write!(writer, "\x1B[34m")?;
                    write!(writer, "{}(", span.name())?;
                    // 重置字体颜色
                    write!(writer, "\x1B[0m")?;
                    // 打印 span 的字段（参数）
                    let extensions = span.extensions();
                    if let Some(fields) = extensions.get::<fmt::FormattedFields<N>>() {
                        write!(writer, "{}", fields)?;
                    }
                    // 设置字体颜色为蓝色
                    write!(writer, "\x1B[34m")?;
                    write!(writer, ")")?;
                    // 重置字体颜色
                    write!(writer, "\x1B[0m")?;
                }
            }
        }

        // 重置字体颜色
        write!(writer, "\x1B[0m")?;

        writeln!(writer)
    }
}

macro_rules! creat_console_layer {
    ($console_time_format:expr, $show_spans:expr) => {
        fmt::layer()
            // .with_timer(ChronoLocal::new("%H:%M:%S%.6f".to_string()))
            // .with_target(false)
            // .pretty()
            .event_format(CustomConsoleFormatter::new(
                $console_time_format,
                $show_spans,
            ))
            .with_writer(std::io::stdout)
    };
}

macro_rules! creat_file_layer {
    ($file_time_format:expr,$non_blocking:expr) => {
        fmt::layer()
            .with_timer(ChronoLocal::new($file_time_format.to_string()))
            .with_file(true)
            .with_line_number(true)
            .json()
            .with_writer($non_blocking)
    };
}

pub type Result<T> = core::result::Result<T, LogError>;

pub struct LogWatcher {
    _file_watcher: FileWatcher,
    pub config_changed_tx: watch::Sender<(LogConfig, HashMap<String, Value>)>,
    reload_join_handle: tokio::task::JoinHandle<()>,
}

impl Drop for LogWatcher {
    fn drop(&mut self) {
        self.reload_join_handle.abort();
    }
}

impl LogWatcher {
    pub async fn new() -> Result<Self> {
        let AppEnv { app_dir, .. } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;
        let (config_changed_tx, mut config_changed_rx) =
            watch::channel((LogConfig::default(), HashMap::new()));
        let (config, files) = build_log_cfg(app_dir).await?;
        let base_config: BaseConfig = config
            .clone()
            .try_deserialize()
            .map_err(CfgError::Deserialize)?;
        let watch_debounce_delay = base_config.watch_debounce_delay;
        let LogConfig {
            level,
            modules,
            console_time_format,
            file_time_format,
            show_spans,
            rotation,
        } = deserialize_config(config.clone()).await?;

        // 创建环境过滤器，支持 RUST_LOG 环境变量和模块级别配置
        let env_filter = create_env_filter(level.clone(), &modules);
        let (env_filter_layer, env_layer_reload_handle) = reload::Layer::new(env_filter);

        // 控制台输出层
        let console_layer = creat_console_layer!(console_time_format, show_spans);
        let (console_layer, console_layer_reload_handle) = reload::Layer::new(console_layer);

        // 文件输出层
        let AppEnv {
            app_dir,
            app_file_name,
            ..
        } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;
        let log_dir_path = app_dir.join("log");
        let log_dir = log_dir_path.to_string_lossy().to_string();
        let file_appender = RollingFileAppender::builder()
            .rotation(rotation.clone()) // 滚动策略
            .filename_prefix(format!("{}.log", app_file_name)) // 文件名前缀
            .filename_suffix("json") // 文件后缀，如 "log", "txt" 等
            .build(log_dir_path) // 日志目录
            .map_err(|e| LogError::CreateFileAppender(e))?;
        let (non_blocking, log_guard) = tracing_appender::non_blocking(file_appender);
        let file_layer = creat_file_layer!(file_time_format, non_blocking);
        {
            let mut log_guard_write_lock =
                LOG_GUARD.write().map_err(|_| LogError::SetLogGuard())?;
            *log_guard_write_lock = Some(log_guard); // 解决锁在初始化方法结束后被提前释放导致后续日志不能输出
        }
        let (file_layer, file_layer_reload_handle) = reload::Layer::new(file_layer);

        tracing_subscriber::registry()
            .with(env_filter_layer)
            .with(console_layer) // 控制台输出层
            .with(file_layer) // 文件输出层
            .init();
        debug!("初始化日志成功");

        let files_clone = files.clone();
        // 监听重新加载日志任务
        let reload_join_handle = tokio::spawn(async move {
            info!("watching log config: {:?}", files_clone);
            loop {
                match config_changed_rx.changed().await {
                    Ok(_) => {
                        let (log_config, _changed) = config_changed_rx.borrow().clone();
                        let LogConfig {
                            level,
                            modules,
                            console_time_format,
                            show_spans,
                            file_time_format,
                            rotation,
                        } = log_config;
                        // 应用新配置
                        env_layer_reload_handle
                            .modify(|filter| {
                                *filter = create_env_filter(level, &modules);
                            })
                            .expect("reload log config error");

                        console_layer_reload_handle
                            .modify(|layer| {
                                *layer = creat_console_layer!(console_time_format, show_spans);
                            })
                            .expect("reload console config error");

                        file_layer_reload_handle
                            .modify(|layer| {
                                // 重新创建文件appender
                                let file_appender = RollingFileAppender::builder()
                                    .rotation(rotation.clone())
                                    .filename_prefix(format!("{}.log", app_file_name))
                                    .filename_suffix("json")
                                    .build(Path::new(log_dir.as_str()))
                                    .expect("create file appender error");
                                let (non_blocking, log_guard) =
                                    tracing_appender::non_blocking(file_appender);

                                *layer = creat_file_layer!(file_time_format, non_blocking);

                                // 更新全局guard
                                let mut guard = LOG_GUARD.write().expect("write log guard");
                                *guard = Some(log_guard);
                            })
                            .expect("reload file config error");
                    }
                    Err(err) => {
                        info!("watch log config error: {:?}", err);
                        break;
                    }
                }
            }
            info!("exit watching log config: {:?}", files_clone);
        });

        // 监听日志配置文件变化
        let last_config = Arc::new(ArcSwap::from_pointee(config.clone()));
        let config_changed_tx_clone = config_changed_tx.clone();
        let file_watcher = watch_file_changed(files.clone(), watch_debounce_delay, move |_| {
            let config_changed_tx_clone = config_changed_tx_clone.clone();
            let last = Arc::clone(&last_config);
            let old_config = last.load_full();
            async move {
                match build_log_cfg(app_dir).await {
                    Ok((new_config, _)) => {
                        let changed = diff_config(&old_config, &new_config);
                        if !changed.is_empty() {
                            info!("log config changed: {:?}", changed);
                            last.store(Arc::new(new_config.clone()));
                            match deserialize_config::<LogConfig>(new_config).await {
                                Ok(log_config) => {
                                    if let Err(e) =
                                        config_changed_tx_clone.send((log_config, changed))
                                    {
                                        error!("send log config error: {:?}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("deserialize log config error: {:?}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("build log config error: {:?}", e);
                    }
                }
                Ok(())
            }
        })?;

        Ok(Self {
            _file_watcher: file_watcher,
            config_changed_tx,
            reload_join_handle,
        })
    }
}

async fn build_log_cfg(app_dir: &PathBuf) -> crate::cfg::Result<(Config, Vec<String>)> {
    build_cfg(app_dir, "LOG", None, "log", None).await
}

fn create_env_filter(level: String, modules: &HashMap<String, String>) -> EnvFilter {
    // 如果 RUST_LOG 存在就用它作为 base level,否则用配置文件里的 level
    let mut filter_string = env::var("RUST_LOG").unwrap_or(level);

    for (module, module_level) in modules {
        filter_string.push_str(&format!(",{}={}", module, module_level));
    }

    EnvFilter::new(filter_string)
}
