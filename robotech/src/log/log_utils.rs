use crate::cfg::{CfgError, build_config, watch_config_file};
use crate::env::{APP_ENV, AppEnv, EnvError};
use crate::log::{LogConfig, LogError};
use log::{debug, warn};
use std::env;
use std::path::Path;
use std::sync::{RwLock, mpsc};
use std::time::Duration;
use tokio::time::interval;
use tracing::instrument;
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
use tracing_subscriber::{EnvFilter, fmt, reload};

/// 日志文件输出锁
/// 解决锁在初始化方法结束后被提前释放导致后续日志不能输出
static LOG_GUARD: RwLock<Option<WorkerGuard>> = RwLock::new(None);

struct CustomConsoleFormatter {
    timer_format: String,
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

/// 初始化日志
#[instrument(level = "debug", err)]
pub fn init_log() -> Result<(), LogError> {
    let (
        LogConfig {
            level,
            console_time_format,
            file_time_format,
            show_spans,
            rotation,
        },
        files,
    ) = build_log_config()?;

    // 创建环境过滤器，支持 RUST_LOG 环境变量
    let env_filter = create_env_filter(level);
    let (env_filter_layer, env_layer_reload_handle) = reload::Layer::new(env_filter);

    // 控制台输出层
    let console_layer = fmt::layer()
        // .with_timer(ChronoLocal::new("%H:%M:%S%.6f".to_string()))
        // .with_target(false)
        // .pretty()
        .event_format(CustomConsoleFormatter::new(console_time_format, show_spans))
        .with_writer(std::io::stdout);
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
    let file_layer = fmt::layer()
        .with_timer(ChronoLocal::new(file_time_format.to_string()))
        .with_file(true)
        .with_line_number(true)
        .json()
        .with_writer(non_blocking);
    {
        let mut log_guard_write_lock = LOG_GUARD.write().map_err(|_| LogError::SetLogGuard())?;
        *log_guard_write_lock = Some(log_guard); // 解决锁在初始化方法结束后被提前释放导致后续日志不能输出
    }
    let (file_layer, file_layer_reload_handle) = reload::Layer::new(file_layer);

    tracing_subscriber::registry()
        .with(env_filter_layer)
        .with(console_layer) // 控制台输出层
        .with(file_layer) // 文件输出层
        .init();
    debug!("初始化日志成功");

    debug!("watch log config file...");
    tokio::spawn(async move {
        let (_watcher, receiver) = watch_config_file(files).expect("watch log config file error");

        // 创建一个1秒间隔的定时器
        let mut interval = interval(Duration::from_secs(1));
        loop {
            // 等待下一个时间点
            interval.tick().await;
            // 使用 try_recv 非阻塞检查
            match receiver.try_recv() {
                Ok(event_result) => {
                    debug!("log config file changed, reload log config...");

                    match event_result {
                        Ok(events) => {
                            // 处理文件事件
                            for event in events {
                                debug!("file event: {:?}", event);
                            }

                            // 重新加载配置
                            let (
                                LogConfig {
                                    level,
                                    console_time_format,
                                    show_spans,
                                    file_time_format,
                                    rotation,
                                },
                                _,
                            ) = build_log_config().expect("build log config error");

                            // 应用新配置
                            env_layer_reload_handle
                                .modify(|filter| {
                                    *filter = create_env_filter(level);
                                })
                                .expect("reload log config error");

                            console_layer_reload_handle
                                .modify(|layer| {
                                    *layer = fmt::layer()
                                        .event_format(CustomConsoleFormatter::new(
                                            console_time_format,
                                            show_spans,
                                        ))
                                        .with_writer(std::io::stdout);
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

                                    *layer = fmt::layer()
                                        .with_timer(ChronoLocal::new(file_time_format.to_string()))
                                        .with_file(true)
                                        .with_line_number(true)
                                        .json()
                                        .with_writer(non_blocking);

                                    // 更新全局guard
                                    let mut guard = LOG_GUARD.write().expect("write log guard");
                                    *guard = Some(log_guard);
                                })
                                .expect("reload file config error");
                        }
                        Err(e) => {
                            warn!("error receiving file events: {:?}", e);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // 没有消息，继续下一次循环
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // 通道关闭
                    debug!("config watcher channel closed, exiting watcher loop");
                    break;
                }
            }
        }

        debug!("config watcher task finished");
    });

    Ok(())
}

fn build_log_config() -> Result<(LogConfig, Vec<String>), CfgError> {
    build_config("LOG", Some("log"), None)
}

fn create_env_filter(level: String) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level))
}
