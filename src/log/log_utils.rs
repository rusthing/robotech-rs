use crate::env::ENV;
use log::info;
use std::{env, fs};
use std::sync::OnceLock;
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
        let normalized_meta = event.normalized_metadata();
        let meta = normalized_meta.as_ref().unwrap_or_else(|| event.metadata());

        // 根据日志级别添加不同颜色
        let level = event.metadata().level();
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
        // write!(writer, "\x1B[39m")?;
        // 设置字体颜色
        let visitor = DefaultFields::default();
        visitor.format_fields(writer.by_ref(), event)?;
        write!(writer, "\x1B[0m")?;

        write!(writer, " \x1B[1;93m-\x1B[0m ")?;

        // 获取文件和行号信息，添加色彩
        // 设置字体颜色为蓝色
        write!(writer, "\x1B[34m")?;
        if let (Some(file_path), Some(line_number)) = (meta.file(), meta.line()) {
            let absolute_path = env::current_dir().unwrap().join(file_path);
            let path = format!("{}:{}", absolute_path.display(), line_number);
            let label = format!("{}:{}", file_path, line_number);
            // writeln!(writer)?;
            write!(
                writer,
                "\x1B]8;;file://{}\x1B\\{}\x1B]8;;\x1B\\",
                path, label
            )?;
        }
        // 重置字体颜色
        write!(writer, "\x1B[0m")?;

        writeln!(writer)
    }
}

/// 初始化日志
pub fn init_log() -> Result<(), std::io::Error> {
    // 创建环境过滤器，支持 RUST_LOG 环境变量
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));

    // 控制台输出层
    let console_layer = tracing_subscriber::fmt::layer()
        // .with_timer(ChronoLocal::new("%H:%M:%S%.6f".to_string()))
        // .with_target(false)
        // .pretty()
        .event_format(CustomFormatter::new("%H:%M:%S%.6f".to_string()))
        .with_writer(std::io::stdout);

    // 文件输出层
    let env = ENV.get().unwrap();
    let log_dir = env.app_dir.join("log");
    fs::create_dir_all(log_dir.as_path())?;
    let log_file_name_prefix = format!("{}.log", env.app_file_name);
    let file_appender = tracing_appender::rolling::hourly(log_dir, log_file_name_prefix);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // 解决锁在初始化方法结束后被提前释放导致后续日志不能输出
    LOG_GUARD.set(guard).map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Failed to set guard"))?;
    let file_layer = fmt::layer()
        .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.6f".to_string()))
        .with_file(true)
        .with_line_number(true)
        .json()
        .with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer) // 控制台输出层
        .with(file_layer) // 文件输出层
        .init();
    info!("初始化日志成功");

    Ok(())
}
