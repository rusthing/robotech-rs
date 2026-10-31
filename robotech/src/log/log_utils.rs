use crate::cfg::{BaseConfig, deserialize_config};
use crate::cfg::{CfgError, build_cfg};
use crate::env::{APP_ENV, AppEnv, EnvError};
use crate::log::{
    DEFAULT_LOG_RETAIN_DAYS, LocalRollingFile, LogConfig, LogError, LogTimeZone, MuteFilter,
    max_log_files_for_days,
};
use arc_swap::ArcSwap;
use chrono::{Local, Utc};
use config::{Config, Value};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::Rotation;
use tracing_core::{Event, Level, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::fmt::format::{DefaultFields, Writer};
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt, reload};
use wheel_rs::config_utils::diff_config;
use wheel_rs::file_utils::{FileWatcher, watch_file_changed};

/// 日志文件输出锁。
///
/// 解决 `WorkerGuard` 在初始化方法结束后被提前释放导致后续日志无法输出的问题。
static LOG_GUARD: RwLock<Option<WorkerGuard>> = RwLock::new(None);

/// 自定义控制台日志格式化器。
///
/// 在控制台输出中按日志级别设置不同颜色、显示时间、级别、target、
/// 文件行号（可点击超链接），并可选打印 span 链（函数名与参数）。
struct CustomConsoleFormatter {
    /// 时间格式
    timer_format: String,
    /// 是否打印 span 链（包括函数名和参数，需 #[instrument] 配合）
    show_spans: bool,
}

impl CustomConsoleFormatter {
    /// 创建自定义控制台格式化器。
    ///
    /// ## 参数
    /// * `timer_format` - 时间格式字符串
    /// * `show_spans` - 是否打印 span 链
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

        let time_str = Utc::now()
            .with_timezone(&Local)
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

/// 构建控制台输出层宏
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

/// 构建文件输出层宏（JSON 格式，带时间戳、文件名与行号）
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

/// 日志模块操作结果类型
pub type Result<T> = core::result::Result<T, LogError>;

/// # 日志监听器
///
/// 初始化 tracing 日志系统（控制台彩色输出 + JSON 文件输出），并监听日志配置
/// 变化，在配置变更时热更新日志级别、输出格式与文件滚动策略。
///
/// 通过 `config_changed_tx` 对外发布日志配置变更通知。
pub struct LogWatcher {
    _file_watcher: FileWatcher,
    pub config_changed_tx: watch::Sender<(LogConfig, HashMap<String, Value>)>,
    reload_join_handle: tokio::task::JoinHandle<()>,
}

impl Drop for LogWatcher {
    /// 退出时 abort 后台日志热更新任务
    fn drop(&mut self) {
        self.reload_join_handle.abort();
    }
}

impl LogWatcher {
    /// # 初始化日志系统
    ///
    /// 读取环境中的应用目录，构建日志配置（`LOG` 配置段），初始化 tracing 日志系统：
    /// - 控制台彩色输出层
    /// - JSON 格式文件输出层（按配置滚动）
    ///
    /// 随后启动两个后台任务：监听日志配置变更并热更新、监听日志配置文件变化并重载。
    ///
    /// ## 返回值
    /// 返回 `Ok(LogWatcher)`；初始化失败时返回 `Err(LogError)`。
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
            tz_offset,
            max_log_files,
            mute_targets,
            mute_keywords,
        } = deserialize_config(config.clone()).await?;

        // 日志屏蔽规则（target 前缀 + 关键字），配置热更新时整体替换
        let mute_targets = Arc::new(ArcSwap::from_pointee(mute_targets));
        let mute_keywords = Arc::new(ArcSwap::from_pointee(normalize_mute_keywords(
            mute_keywords,
        )));

        // 创建环境过滤器，支持 RUST_LOG 环境变量和模块级别配置
        let env_filter = create_env_filter(level.clone(), &modules);
        let (env_filter_layer, env_layer_reload_handle) = reload::Layer::new(env_filter);

        // 控制台输出层：先包 reload 再叠加屏蔽过滤器，
        // 保证热更新替换的是内层格式化器而不是整条过滤链
        let (console_layer, console_layer_reload_handle) =
            reload::Layer::new(creat_console_layer!(console_time_format, show_spans));
        let console_layer = console_layer.with_filter(MuteFilter::new(
            Arc::clone(&mute_targets),
            Arc::clone(&mute_keywords),
        ));

        // 文件输出层
        //
        // 文件名中的时间戳按配置时区（缺省跟随系统本地时区）生成，与文件 mtime
        // 使用同一时区，避免出现「文件名比 mtime 慢 8 小时」的 UTC/本地时区错位。
        let AppEnv {
            app_dir,
            app_file_name,
            ..
        } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;
        let log_dir_path = app_dir.join("log");
        let log_dir = log_dir_path.to_string_lossy().to_string();
        let log_prefix = format!("{}.log", app_file_name);
        let log_suffix = "json";
        let log_time_zone = resolve_time_zone(tz_offset);
        let file_appender = build_file_appender(
            &log_dir_path,
            &log_prefix,
            log_suffix,
            &rotation,
            log_time_zone,
            max_log_files,
        )?;
        // tracing 尚未 init，先把自检结果采集下来，init 之后再输出
        let log_naming_probe =
            LogNamingProbe::collect(&file_appender, &log_dir_path, &log_prefix, log_suffix);
        let (non_blocking, log_guard) = tracing_appender::non_blocking(file_appender);
        let (file_layer, file_layer_reload_handle) =
            reload::Layer::new(creat_file_layer!(file_time_format, non_blocking));
        let file_layer = file_layer.with_filter(MuteFilter::new(
            Arc::clone(&mute_targets),
            Arc::clone(&mute_keywords),
        ));
        {
            let mut log_guard_write_lock =
                LOG_GUARD.write().map_err(|_| LogError::SetLogGuard())?;
            *log_guard_write_lock = Some(log_guard); // 解决锁在初始化方法结束后被提前释放导致后续日志不能输出
        }

        tracing_subscriber::registry()
            .with(env_filter_layer)
            .with(console_layer) // 控制台输出层
            .with(file_layer) // 文件输出层
            .init();
        debug!("初始化日志成功");
        // 日志文件名时区自检：输出实际生效的偏移与文件名样例，并提示按其它时区命名的历史文件
        log_naming_probe.report();

        let files_clone = files.clone();
        let mute_targets_shared = Arc::clone(&mute_targets);
        let mute_keywords_shared = Arc::clone(&mute_keywords);
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
                            tz_offset,
                            max_log_files,
                            mute_targets,
                            mute_keywords,
                        } = log_config;
                        // 更新屏蔽规则（过滤器通过共享句柄读取，无需重建 layer）
                        mute_targets_shared.store(Arc::new(mute_targets));
                        mute_keywords_shared
                            .store(Arc::new(normalize_mute_keywords(mute_keywords)));
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
                                // 重新创建文件appender（时区随之更新）
                                let log_time_zone = resolve_time_zone(tz_offset);
                                let file_appender = build_file_appender(
                                    Path::new(log_dir.as_str()),
                                    &format!("{}.log", app_file_name),
                                    "json",
                                    &rotation,
                                    log_time_zone,
                                    max_log_files,
                                )
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

/// 构建日志配置对象。
///
/// 仅构建日志配置（`log` 配置文件），不涉及配置中心初始化。
async fn build_log_cfg(app_dir: &PathBuf) -> crate::cfg::Result<(Config, Vec<String>)> {
    build_cfg(app_dir, "LOG", None, "log", None).await
}

/// 解析日志文件名时间戳使用的时区。
///
/// 优先级：配置项 `tz-offset` > 环境变量 `LOG_TZ_OFFSET` > 系统本地时区。
/// 偏移超出 `±86399` 秒视为非法，回退到系统本地时区；实际生效值会在启动自检日志中打印，
/// 便于与 `date +%z` 对照。
///
/// ## 参数
/// * `tz_offset` - 配置中的时区偏移（秒）
///
/// ## 返回值
/// 实际使用的时区
fn resolve_time_zone(tz_offset: Option<i32>) -> LogTimeZone {
    let offset = tz_offset.or_else(|| {
        env::var("LOG_TZ_OFFSET")
            .ok()
            .and_then(|value| value.trim().parse::<i32>().ok())
    });
    match offset {
        Some(secs) if (-86_399..=86_399).contains(&secs) => LogTimeZone::Fixed(secs),
        _ => LogTimeZone::Local,
    }
}

/// 构建本地时区滚动文件 appender。
///
/// 未配置 `max-log-files` 时按 [`DEFAULT_LOG_RETAIN_DAYS`] 天换算保留个数；
/// 配置为 `0` 表示关闭清理。
///
/// ## 参数
/// * `log_dir` - 日志目录
/// * `prefix` - 文件名前缀（如 `gatex-svr.log`）
/// * `suffix` - 文件名后缀（如 `json`）
/// * `rotation` - 滚动策略
/// * `time_zone` - 文件名时间戳使用的时区
/// * `max_log_files` - 保留文件个数上限
///
/// ## 返回值
/// 成功返回 appender，失败返回 [`LogError`]
fn build_file_appender(
    log_dir: &Path,
    prefix: &str,
    suffix: &str,
    rotation: &Rotation,
    time_zone: LogTimeZone,
    max_log_files: Option<usize>,
) -> Result<LocalRollingFile> {
    let mut builder = LocalRollingFile::builder()
        .rotation(rotation.clone())
        .filename_prefix(prefix.to_string())
        .filename_suffix(suffix.to_string())
        .time_zone(time_zone);
    // 缺省按保留天数换算；显式配 0 视为关闭清理
    let max_files = match max_log_files {
        Some(0) => None,
        Some(max_files) => Some(max_files),
        None => max_log_files_for_days(rotation, DEFAULT_LOG_RETAIN_DAYS),
    };
    if let Some(max_files) = max_files {
        builder = builder.max_log_files(max_files);
    }
    builder
        .build(log_dir)
        .map_err(LogError::CreateLocalRollingFile)
}

/// 日志文件名时区自检结果。
///
/// tracing 初始化之前无法输出日志，因此先把结论采集到该结构体，`init` 之后再统一打印。
struct LogNamingProbe {
    /// 实际生效的时区偏移（秒）
    offset_secs: i32,
    /// 当前周期的时间戳样例
    sample: String,
    /// 扫描到的日志文件总数
    total: usize,
    /// 文件名时间戳与 mtime 不在同一滚动周期的文件数
    mismatched: usize,
    /// 不匹配样例（最多 3 条）
    examples: Vec<String>,
}

impl LogNamingProbe {
    /// 采集自检信息。
    ///
    /// 逐个比对已有日志文件的「文件名时间戳」与「mtime 换算出的时间戳」：
    /// 二者由同一套时区与滚动策略计算，落在同一周期才算一致。不一致说明该文件
    /// 是按其它时区命名的（例如旧版本按 UTC 生成，东八区下相差 8 小时）。
    ///
    /// ## 参数
    /// * `appender` - 已构建的 appender，提供时区与时间戳换算
    /// * `log_dir` - 日志目录
    /// * `prefix` - 文件名前缀
    /// * `suffix` - 文件名后缀
    ///
    /// ## 返回值
    /// 自检结果
    fn collect(appender: &LocalRollingFile, log_dir: &Path, prefix: &str, suffix: &str) -> Self {
        let mut total = 0usize;
        let mut mismatched = 0usize;
        let mut examples = Vec::new();
        let suffix_part = format!(".{}", suffix);
        if let Ok(entries) = fs::read_dir(log_dir) {
            for entry in entries.flatten() {
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                let file_name = entry.file_name().to_string_lossy().to_string();
                // 只认 {prefix}.{stamp}.{suffix} 这种形态
                let Some(rest) = file_name
                    .strip_prefix(prefix)
                    .and_then(|rest| rest.strip_prefix('.'))
                else {
                    continue;
                };
                let Some(stamp) = rest.strip_suffix(suffix_part.as_str()) else {
                    continue;
                };
                let Some(modified) = metadata.modified().ok() else {
                    continue;
                };
                total += 1;
                let expected = appender.stamp_of(modified);
                if expected != stamp {
                    mismatched += 1;
                    if examples.len() < 3 {
                        examples.push(format!("{file_name}（文件名={stamp}, mtime={expected}）"));
                    }
                }
            }
        }
        Self {
            offset_secs: appender.time_zone_offset_secs(),
            sample: appender.now_stamp(),
            total,
            mismatched,
            examples,
        }
    }

    /// 输出自检结果。
    ///
    /// 正常情况下 INFO 一行即可确认时区；发现历史文件时区不一致时额外打 WARN。
    fn report(&self) {
        info!(
            offset_secs = self.offset_secs,
            sample = %self.sample,
            "日志文件名时区"
        );
        if self.mismatched > 0 {
            warn!(
                mismatched = self.mismatched,
                total = self.total,
                examples = ?self.examples,
                "存在文件名时间戳与 mtime 不在同一滚动周期的日志文件，可能是旧版本按 UTC 命名留下的，建议改名或归档"
            );
        }
    }
}

/// 创建 tracing 环境过滤器。
///
/// 优先使用 `RUST_LOG` 环境变量作为基础级别，未设置时使用配置文件中的 `level`；
/// 再叠加 `modules` 中按模块（target）覆盖的日志级别。
fn create_env_filter(level: String, modules: &HashMap<String, String>) -> EnvFilter {
    // 如果 RUST_LOG 存在就用它作为 base level,否则用配置文件里的 level
    let mut filter_string = env::var("RUST_LOG").unwrap_or(level);

    for (module, module_level) in modules {
        filter_string.push_str(&format!(",{}={}", module, module_level));
    }

    EnvFilter::new(filter_string)
}

/// 把屏蔽关键字统一转成小写。
///
/// 过滤器对每条命中 target 的日志只把事件文本转小写一次，关键字侧预先转好，
/// 避免逐关键字重复转换。
///
/// ## 参数
/// * `keywords` - 配置中的原始关键字列表
///
/// ## 返回值
/// 全部为小写的关键字列表
fn normalize_mute_keywords(keywords: Vec<String>) -> Vec<String> {
    keywords.into_iter().map(|k| k.to_lowercase()).collect()
}
