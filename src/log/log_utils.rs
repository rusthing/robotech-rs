use crate::env::{Env, EnvError, ENV};
use crate::log::LogError;
use log::debug;
use std::env;
use std::sync::OnceLock;
use tracing_appender::rolling::RollingFileAppender;
use tracing_core::{Event, Level, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::fmt::format::{DefaultFields, Writer};
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

/// 日志文件输出锁
/// 解决锁在初始化方法结束后被提前释放导致后续日志不能输出
static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

struct CustomFormatter {
    timer_format: String,
}

impl CustomFormatter {
    pub fn new(timer_format: String) -> Self {
        Self { timer_format }
    }
}

impl<S, N> FormatEvent<S, N> for CustomFormatter
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
        // 设置字体颜色为蓝色
        write!(writer, "\x1B[34m")?;

        // 获取文件和行号信息
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
        if let Some(scope) = _ctx.event_scope() {
            for span in scope.from_root() {
                // 添加一个箭头"->"
                write!(writer, " \x1B[1;93m->\x1B[0m ")?;
                // 重置字体颜色
                write!(writer, "\x1B[0m")?;
                write!(writer, "{}(", span.name())?;
                // 打印 span 的字段（参数）
                let extensions = span.extensions();
                if let Some(fields) = extensions.get::<fmt::FormattedFields<N>>() {
                    write!(writer, "{}", fields)?;
                }
                write!(writer, ")")?;
            }
        }

        // 重置字体颜色
        write!(writer, "\x1B[0m")?;

        writeln!(writer)
    }
}

/// 初始化日志
pub fn init_log() -> Result<(), LogError> {
    // 创建环境过滤器，支持 RUST_LOG 环境变量
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // 控制台输出层
    let console_layer = tracing_subscriber::fmt::layer()
        // .with_timer(ChronoLocal::new("%H:%M:%S%.6f".to_string()))
        // .with_target(false)
        // .pretty()
        .event_format(CustomFormatter::new("%H:%M:%S%.6f".to_string()))
        .with_writer(std::io::stdout);

    // 文件输出层
    let Env {
        app_dir,
        app_file_name,
        log_rotation,
        ..
    } = ENV.get().ok_or(EnvError::GetEnv())?;
    let log_dir = app_dir.join("log");
    let file_appender = RollingFileAppender::builder()
        .rotation(log_rotation.clone()) // 滚动策略：每天
        .filename_prefix(format!("{}.log", app_file_name)) // 文件名前缀
        .filename_suffix("json") // 文件后缀，如 "log", "txt" 等
        .build(log_dir) // 日志目录
        .map_err(|e| LogError::CreateFileAppender(e))?;
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    LOG_GUARD.set(guard).map_err(|_| LogError::SetLogGuard())?; // 解决锁在初始化方法结束后被提前释放导致后续日志不能输出
    let file_layer = fmt::layer()
        .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.6f".to_string()))
        .with_file(true)
        .with_line_number(true)
        .json()
        .with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer) // 文件输出层
        .with(console_layer) // 控制台输出层
        .init();
    debug!("初始化日志成功");
    Ok(())
}
