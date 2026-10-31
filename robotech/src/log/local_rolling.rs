//! # 本地时区滚动日志文件
//!
//! 用于替代 `tracing_appender::rolling::RollingFileAppender`。
//!
//! ## 为什么不用上游实现
//!
//! `tracing-appender` 的滚动文件名固定按 **UTC** 生成（源码中直接调用
//! `OffsetDateTime::now_utc()`，且 `Builder` 没有任何时区参数，`now` 字段仅在
//! `#[cfg(test)]` 下可注入），而文件 mtime 由内核按**系统本地时区**写入。
//! 两者相差一个时区偏移（东八区为 8 小时），表现为「文件名比 mtime 慢 8 小时」。
//! 由于没有任何注入口，设置 `TZ` 环境变量也无法改变该行为。
//!
//! 本实现用 `chrono` 解析时区（`chrono::Local` 走 libc `localtime_r`，多线程下可用，
//! 不像 `time` crate 的 `current_local_offset()` 需要 `--cfg unsound_local_offset`），
//! 文件名格式与上游**逐字符一致**，因此不会影响既有的日志采集、归档与检索规则。
//!
//! ## 时区来源
//!
//! - [`LogTimeZone::Local`]：跟随系统本地时区（默认）
//! - [`LogTimeZone::Fixed`]：由配置显式指定的固定偏移（秒），用于容器内无 tzdata
//!   或需要与宿主机时区对齐的场景
//!
//! ## 文件名格式
//!
//! | 滚动策略 | 文件名 |
//! | --- | --- |
//! | `MINUTELY` | `{prefix}.2026-10-31-10-31.{suffix}` |
//! | `HOURLY` | `{prefix}.2026-10-31-10.{suffix}` |
//! | `DAILY` / `WEEKLY` | `{prefix}.2026-10-31.{suffix}` |
//! | `NEVER` | `{prefix}.{suffix}`（不带时间戳） |

use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, Timelike, Utc};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing_appender::rolling::Rotation;

/// 分钟级滚动的时间戳格式
const MINUTELY_PATTERN: &str = "%Y-%m-%d-%H-%M";
/// 小时级滚动的时间戳格式
const HOURLY_PATTERN: &str = "%Y-%m-%d-%H";
/// 天级/周级滚动的时间戳格式
const DAILY_PATTERN: &str = "%Y-%m-%d";

/// 日志文件名时间戳使用的时区。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogTimeZone {
    /// 跟随系统本地时区。
    ///
    /// 容器内若未挂载 `/etc/localtime` 且没有 tzdata，会静默退化为 UTC，
    /// 此时启动自检日志里的 `offset-secs` 会是 0，可用于确认。
    Local,
    /// 固定偏移（秒）。东八区为 `28800`。
    Fixed(i32),
}

impl LogTimeZone {
    /// 返回当前生效的时区偏移（秒）。
    ///
    /// ## 返回值
    /// 相对 UTC 的偏移秒数，例如东八区返回 `28800`。
    pub fn offset_secs(&self) -> i32 {
        match self {
            LogTimeZone::Local => Local::now().offset().local_minus_utc(),
            LogTimeZone::Fixed(secs) => *secs,
        }
    }
}

/// 本地时区滚动日志文件。
///
/// 实现 [`std::io::Write`]，配合 `tracing_appender::non_blocking` 使用；
/// 每次写入前检查滚动周期是否变化，变化则切换到新周期的文件。
#[derive(Debug)]
pub struct LocalRollingFile {
    /// 日志目录
    dir: PathBuf,
    /// 文件名前缀（不含结尾的点）
    prefix: Option<String>,
    /// 文件名后缀（不含开头的点）
    suffix: Option<String>,
    /// 滚动策略
    rotation: Rotation,
    /// 保留的文件个数上限，`None` 表示不清理
    max_files: Option<usize>,
    /// 固定时区偏移，`None` 表示跟随系统本地时区
    fixed: Option<FixedOffset>,
    /// 当前打开的文件：文件名（含时间戳）与句柄
    current: Option<(String, File)>,
}

impl LocalRollingFile {
    /// 创建构建器。
    pub fn builder() -> LocalRollingFileBuilder {
        LocalRollingFileBuilder::default()
    }

    /// 当前打开的文件路径。
    ///
    /// ## 返回值
    /// 已打开时返回文件绝对路径，未打开时返回 `None`。
    pub fn current_path(&self) -> Option<PathBuf> {
        self.current
            .as_ref()
            .map(|(name, _)| self.dir.join(name))
    }

    /// 当前文件文件名中的时间戳部分。
    ///
    /// ## 返回值
    /// 例如 `2026-10-31-10-31`；`Rotation::NEVER` 时返回空串。
    pub fn current_stamp(&self) -> String {
        self.now_stamp()
    }

    /// 当前时刻在文件名中对应的时间戳。
    ///
    /// ## 返回值
    /// 按滚动策略取整后的时间戳字符串，如 `2026-10-31-10-31`。
    pub fn now_stamp(&self) -> String {
        self.stamp_of_datetime(self.now())
    }

    /// 把任意时刻换算成文件名中对应的时间戳。
    ///
    /// ## 参数
    /// * `time` - 待换算的系统时间（通常是文件 mtime）
    ///
    /// ## 返回值
    /// 换算到目标时区并按滚动策略取整后的时间戳字符串
    pub fn stamp_of(&self, time: SystemTime) -> String {
        let utc: DateTime<Utc> = time.into();
        self.stamp_of_datetime(utc.fixed_offset())
    }

    /// 当前生效的时区偏移（秒）。
    ///
    /// ## 返回值
    /// 相对 UTC 的偏移秒数，东八区为 `28800`；跟随本地时区时实时读取。
    pub fn time_zone_offset_secs(&self) -> i32 {
        match self.fixed {
            Some(offset) => offset.local_minus_utc(),
            None => Local::now().offset().local_minus_utc(),
        }
    }

    /// 把时刻换算到目标时区。
    fn to_zone(&self, dt: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
        match self.fixed {
            Some(offset) => dt.with_timezone(&offset),
            None => dt.with_timezone(&Local).fixed_offset(),
        }
    }

    /// 当前时刻（已换算到目标时区）。
    fn now(&self) -> DateTime<FixedOffset> {
        self.to_zone(Utc::now().fixed_offset())
    }

    /// 时间戳格式。
    fn date_pattern(&self) -> &'static str {
        if self.rotation == Rotation::MINUTELY {
            MINUTELY_PATTERN
        } else if self.rotation == Rotation::HOURLY {
            HOURLY_PATTERN
        } else {
            DAILY_PATTERN
        }
    }

    /// 按滚动策略把时刻取整到周期起点。
    fn round(&self, dt: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
        let zero_nanos = |d: DateTime<FixedOffset>| d.with_nanosecond(0).unwrap_or(d);
        if self.rotation == Rotation::MINUTELY {
            dt.with_second(0).map(zero_nanos).unwrap_or(dt)
        } else if self.rotation == Rotation::HOURLY {
            dt.with_minute(0)
                .and_then(|d| d.with_second(0))
                .map(zero_nanos)
                .unwrap_or(dt)
        } else if self.rotation == Rotation::DAILY {
            dt.with_hour(0)
                .and_then(|d| d.with_minute(0))
                .and_then(|d| d.with_second(0))
                .map(zero_nanos)
                .unwrap_or(dt)
        } else if self.rotation == Rotation::WEEKLY {
            // 与上游一致：回退到本周周日 00:00
            let days = i64::from(dt.weekday().num_days_from_sunday());
            let sunday = dt - Duration::days(days);
            sunday
                .with_hour(0)
                .and_then(|d| d.with_minute(0))
                .and_then(|d| d.with_second(0))
                .map(zero_nanos)
                .unwrap_or(sunday)
        } else {
            dt
        }
    }

    /// 生成某个时刻对应的时间戳字符串（先换算到目标时区再取整）。
    fn stamp_of_datetime(&self, dt: DateTime<FixedOffset>) -> String {
        if self.rotation == Rotation::NEVER {
            return String::new();
        }
        let dt = self.to_zone(dt);
        self.round(dt).format(self.date_pattern()).to_string()
    }

    /// 按前缀、时间戳、后缀拼出文件名。
    ///
    /// 拼接规则与 `tracing_appender::rolling::Inner::join_date` 完全一致。
    fn join_name(&self, stamp: &str) -> String {
        let never = self.rotation == Rotation::NEVER;
        match (&self.prefix, &self.suffix) {
            (Some(prefix), Some(suffix)) if never => format!("{}.{}", prefix, suffix),
            (Some(prefix), Some(suffix)) => format!("{}.{}.{}", prefix, stamp, suffix),
            (Some(prefix), None) => prefix.to_string(),
            (None, Some(suffix)) if never => suffix.to_string(),
            (None, Some(suffix)) => format!("{}.{}", stamp, suffix),
            (None, None) => stamp.to_string(),
        }
    }

    /// 打开指定文件（追加模式），必要时先滚动。
    fn roll_to(&mut self, stamp: &str) -> io::Result<()> {
        let name = self.join_name(stamp);
        if let Some((current_name, _)) = self.current.as_ref()
            && current_name == &name
        {
            return Ok(());
        }
        // 关闭旧文件前先 flush，避免丢最后一段缓冲
        if let Some((_, mut old)) = self.current.take() {
            let _ = old.flush();
        }
        let path = self.dir.join(&name);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        self.current = Some((name, file));
        self.prune_old_files();
        Ok(())
    }

    /// 清理超出保留个数的旧文件（按 mtime 从新到旧保留）。
    fn prune_old_files(&self) {
        let Some(max_files) = self.max_files else {
            return;
        };
        // 0 视为关闭清理，避免误删当前正在写的文件
        if max_files == 0 {
            return;
        }
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut files: Vec<(SystemTime, PathBuf)> = entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let metadata = entry.metadata().ok()?;
                // 只清理普通文件，目录与符号链接一律跳过
                if !metadata.is_file() {
                    return None;
                }
                let name = entry.file_name();
                let name = name.to_str()?;
                if let Some(prefix) = &self.prefix
                    && !name.starts_with(prefix.as_str())
                {
                    return None;
                }
                if let Some(suffix) = &self.suffix
                    && !name.ends_with(suffix.as_str())
                {
                    return None;
                }
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                Some((modified, entry.path()))
            })
            .collect();
        if files.len() <= max_files {
            return;
        }
        files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        for (_, path) in files.into_iter().skip(max_files) {
            let _ = fs::remove_file(path);
        }
    }
}

impl Write for LocalRollingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let stamp = self.now_stamp();
        self.roll_to(&stamp)?;
        match self.current.as_mut() {
            Some((_, file)) => file.write(buf),
            // roll_to 成功后必然有 current，此处仅为类型收窄
            None => Err(io::Error::other("log file is not opened")),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.current.as_mut() {
            Some((_, file)) => file.flush(),
            None => Ok(()),
        }
    }
}

/// [`LocalRollingFile`] 的构建器。
#[derive(Debug, Default)]
pub struct LocalRollingFileBuilder {
    /// 滚动策略，默认不滚动
    rotation: Option<Rotation>,
    /// 文件名前缀
    prefix: Option<String>,
    /// 文件名后缀
    suffix: Option<String>,
    /// 保留文件个数上限
    max_files: Option<usize>,
    /// 时区
    time_zone: Option<LogTimeZone>,
}

impl LocalRollingFileBuilder {
    /// 设置滚动策略。
    pub fn rotation(mut self, rotation: Rotation) -> Self {
        self.rotation = Some(rotation);
        self
    }

    /// 设置文件名前缀（时间戳之前，无需带结尾的点）。
    pub fn filename_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// 设置文件名后缀（时间戳之后，无需带开头的点）。
    pub fn filename_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    /// 设置保留的文件个数上限，超出后按 mtime 删除最旧的；`0` 表示关闭清理。
    pub fn max_log_files(mut self, max_files: usize) -> Self {
        self.max_files = Some(max_files);
        self
    }

    /// 设置文件名时间戳使用的时区。
    pub fn time_zone(mut self, time_zone: LogTimeZone) -> Self {
        self.time_zone = Some(time_zone);
        self
    }

    /// 构建 appender，并立即打开当前周期的文件。
    ///
    /// ## 参数
    /// * `directory` - 日志目录，不存在时会被创建
    ///
    /// ## 返回值
    /// 成功返回 [`LocalRollingFile`]；目录创建或文件打开失败返回 [`io::Error`]。
    pub fn build(self, directory: impl AsRef<Path>) -> io::Result<LocalRollingFile> {
        let dir = directory.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        let fixed = match self.time_zone.unwrap_or(LogTimeZone::Local) {
            LogTimeZone::Local => None,
            LogTimeZone::Fixed(secs) => Some(
                FixedOffset::east_opt(secs)
                    .ok_or_else(|| io::Error::other(format!("invalid tz offset: {secs}")))?,
            ),
        };
        let mut appender = LocalRollingFile {
            dir,
            prefix: self.prefix,
            suffix: self.suffix,
            rotation: self.rotation.unwrap_or(Rotation::NEVER),
            max_files: self.max_files,
            fixed,
            current: None,
        };
        let stamp = appender.now_stamp();
        appender.roll_to(&stamp)?;
        Ok(appender)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "robotech-log-test-{}-{}-{:?}",
            tag,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn filename_uses_configured_offset() {
        let dir = temp_dir("offset");
        let appender = LocalRollingFile::builder()
            .rotation(Rotation::MINUTELY)
            .filename_prefix("app.log")
            .filename_suffix("json")
            .time_zone(LogTimeZone::Fixed(8 * 3600))
            .build(&dir)
            .unwrap();
        // UTC 02:31 在东八区应命名为 10:31
        let utc = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 10, 31, 2, 31, 45)
            .unwrap();
        assert_eq!(appender.stamp_of_datetime(utc), "2026-10-31-10-31");
        assert_eq!(appender.time_zone_offset_secs(), 8 * 3600);
        let path = appender.current_path().unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with(".json"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stamp_matches_mtime_in_same_zone() {
        let dir = temp_dir("mtime");
        let offset = FixedOffset::east_opt(8 * 3600).unwrap();
        let mut appender = LocalRollingFile::builder()
            .rotation(Rotation::MINUTELY)
            .filename_prefix("app.log")
            .filename_suffix("json")
            .time_zone(LogTimeZone::Fixed(8 * 3600))
            .build(&dir)
            .unwrap();
        appender.write_all(b"{}\n").unwrap();
        appender.flush().unwrap();
        let path = appender.current_path().unwrap();
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();

        // 文件名里的时间戳换算回时刻后，与 mtime 的差必须小于一个滚动周期
        let stamp = appender.current_stamp();
        let naive = chrono::NaiveDateTime::parse_from_str(&stamp, MINUTELY_PATTERN).unwrap();
        let named_at = offset.from_local_datetime(&naive).unwrap();
        let written_at = DateTime::<Local>::from(mtime).with_timezone(&offset);
        let delta = (written_at - named_at).num_seconds().abs();
        assert!(delta < 60, "文件名 {stamp} 与 mtime {written_at} 相差 {delta}s");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn local_zone_matches_mtime() {
        let dir = temp_dir("local");
        // 不指定 time_zone，走默认的 Local 分支
        let mut appender = LocalRollingFile::builder()
            .rotation(Rotation::MINUTELY)
            .filename_prefix("app.log")
            .filename_suffix("json")
            .build(&dir)
            .unwrap();
        appender.write_all(b"{}\n").unwrap();
        appender.flush().unwrap();
        let path = appender.current_path().unwrap();
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();

        let stamp = appender.current_stamp();
        let naive = chrono::NaiveDateTime::parse_from_str(&stamp, MINUTELY_PATTERN).unwrap();
        let named_at = Local.from_local_datetime(&naive).unwrap();
        let written_at: DateTime<Local> = mtime.into();
        let delta = (written_at - named_at).num_seconds().abs();
        assert!(delta < 60, "文件名 {stamp} 与 mtime {written_at} 相差 {delta}s");
        assert_eq!(
            appender.time_zone_offset_secs(),
            Local::now().offset().local_minus_utc()
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn prune_keeps_newest_files() {
        let dir = temp_dir("prune");
        for i in 0..5 {
            let path = dir.join(format!("app.log.2026-10-3{}.json", i));
            fs::write(&path, b"").unwrap();
        }
        let appender = LocalRollingFile::builder()
            .rotation(Rotation::MINUTELY)
            .filename_prefix("app.log")
            .filename_suffix("json")
            .max_log_files(2)
            .build(&dir)
            .unwrap();
        // build 会打开当前文件并触发一次清理
        let count = fs::read_dir(&dir).unwrap().count();
        assert!(count <= 3, "剩余文件数应被裁剪，实际 {count}");
        assert!(appender.current_path().unwrap().exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn never_rotation_has_no_timestamp() {
        let dir = temp_dir("never");
        let appender = LocalRollingFile::builder()
            .rotation(Rotation::NEVER)
            .filename_prefix("app.log")
            .filename_suffix("json")
            .build(&dir)
            .unwrap();
        let name = appender
            .current_path()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(name, "app.log.json");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn weekly_rounds_to_sunday() {
        let dir = temp_dir("weekly");
        let appender = LocalRollingFile::builder()
            .rotation(Rotation::WEEKLY)
            .filename_prefix("app.log")
            .time_zone(LogTimeZone::Fixed(0))
            .build(&dir)
            .unwrap();
        // 2026-10-31 是周六，周级滚动应回退到 2026-10-25（周日）
        let dt = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 10, 31, 10, 31, 0)
            .unwrap();
        assert_eq!(appender.stamp_of_datetime(dt), "2026-10-25");
        fs::remove_dir_all(&dir).unwrap();
    }
}
