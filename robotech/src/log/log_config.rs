//! # 日志配置模块
//!
//! 定义日志初始化与热更新所需的配置结构体（`LogConfig`）及其默认值。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing_appender::rolling::Rotation;
use wheel_rs::serde::rotation_serde;

/// # 日志配置
///
/// 日志初始化与热更新所需的配置项，支持从配置文件中反序列化（kebab-case 命名）。
///
/// 字段说明：
/// - `level`：全局日志级别
/// - `modules`：按模块（target）覆盖的日志级别
/// - `console_time_format`：控制台日志时间格式
/// - `file_time_format`：文件日志时间格式
/// - `rotation`：日志文件滚动策略
/// - `tz_offset`：日志文件名时间戳所用的时区偏移（秒），缺省跟随系统本地时区
/// - `max_log_files`：日志文件保留个数上限，缺省按 7 天换算，`0` 表示关闭清理
/// - `show_spans`：是否在控制台输出中打印 span 链
/// - `mute_targets`：日志屏蔽生效的 target 前缀列表（为空表示不限制 target）
/// - `mute_keywords`：日志屏蔽关键字（大小写不敏感，命中即丢弃该条日志）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LogConfig {
    /// 日志级别
    #[serde(default = "level_default")]
    pub level: String,
    /// 模块日志级别
    #[serde(default)]
    pub modules: HashMap<String, String>,
    /// 控制台时间格式
    #[serde(default = "console_time_format_default")]
    pub console_time_format: String,
    /// 文件时间格式
    #[serde(default = "file_time_format_default")]
    pub file_time_format: String,
    /// 日志文件滚动策略
    #[serde(with = "rotation_serde", default = "log_rotation_default")]
    pub rotation: Rotation,
    /// 日志文件名时间戳所用的时区偏移（秒），如东八区为 `28800`。
    ///
    /// 缺省（`None`）时跟随系统本地时区；容器内没有 tzdata 时可显式指定。
    #[serde(default)]
    pub tz_offset: Option<i32>,
    /// 日志文件保留个数上限，超出后按 mtime 删除最旧的。
    ///
    /// 缺省（`None`）按 [`DEFAULT_LOG_RETAIN_DAYS`] 天换算：分钟级 10080 个、
    /// 小时级 168 个、天级 7 个、周级 2 个。配置为 `0` 表示关闭清理。
    #[serde(default)]
    pub max_log_files: Option<usize>,
    /// 是否显示spans
    #[serde(default)]
    pub show_spans: bool,
    /// 日志屏蔽生效的 target 前缀列表（为空表示对所有 target 生效，建议显式配置）
    #[serde(default)]
    pub mute_targets: Vec<String>,
    /// 日志屏蔽关键字（大小写不敏感，命中即丢弃该条日志；旧键名 `sql-mute-keywords` 仍可用）
    #[serde(default, alias = "sql-mute-keywords")]
    pub mute_keywords: Vec<String>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: level_default(),
            modules: HashMap::default(),
            console_time_format: console_time_format_default(),
            file_time_format: file_time_format_default(),
            rotation: log_rotation_default(),
            tz_offset: None,
            max_log_files: None,
            show_spans: bool::default(),
            mute_targets: Vec::new(),
            mute_keywords: Vec::new(),
        }
    }
}

/// 默认日志级别为 `info`
fn level_default() -> String {
    "info".to_string()
}

/// 默认控制台时间格式为 `%H:%M:%S%.6f`
fn console_time_format_default() -> String {
    "%H:%M:%S%.6f".to_string()
}

/// 默认文件时间格式为 `%Y-%m-%d %H:%M:%S%.6f`
fn file_time_format_default() -> String {
    "%Y-%m-%d %H:%M:%S%.6f".to_string()
}

/// 默认日志保留天数
pub const DEFAULT_LOG_RETAIN_DAYS: u32 = 7;

/// 把保留天数按滚动策略换算成文件个数上限。
///
/// 滚动越细，同样天数对应的文件越多：
///
/// | 滚动策略 | 7 天对应的文件数 |
/// | --- | --- |
/// | `minutely` | 10080 |
/// | `hourly` | 168 |
/// | `daily` | 7 |
/// | `weekly` | 2 |
/// | `never` | 不清理（返回 `None`） |
///
/// 周级滚动下单个文件就覆盖 7 天，若只保留 1 个会在跨周时立刻丢掉上一周，
/// 因此至少保留当前周与上一周共 2 个。
///
/// ## 参数
/// * `rotation` - 滚动策略
/// * `days` - 需要保留的天数
///
/// ## 返回值
/// 文件个数上限；`Rotation::NEVER` 不滚动，返回 `None`
pub fn max_log_files_for_days(rotation: &Rotation, days: u32) -> Option<usize> {
    let per_day = match *rotation {
        Rotation::MINUTELY => 24 * 60,
        Rotation::HOURLY => 24,
        Rotation::DAILY => 1,
        Rotation::WEEKLY => return Some(usize::try_from(days / 7).unwrap_or(1).max(1) + 1),
        Rotation::NEVER => return None,
    };
    Some((days as usize).saturating_mul(per_day))
}

/// 默认日志滚动策略为按小时滚动
fn log_rotation_default() -> Rotation {
    Rotation::HOURLY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_days_to_file_count() {
        assert_eq!(max_log_files_for_days(&Rotation::MINUTELY, 7), Some(10_080));
        assert_eq!(max_log_files_for_days(&Rotation::HOURLY, 7), Some(168));
        assert_eq!(max_log_files_for_days(&Rotation::DAILY, 7), Some(7));
        // 周级不足一个周期，至少保留 2 个
        assert_eq!(max_log_files_for_days(&Rotation::WEEKLY, 7), Some(2));
        // 不滚动就不清理
        assert_eq!(max_log_files_for_days(&Rotation::NEVER, 7), None);
    }

    #[test]
    fn default_retain_days_is_seven() {
        assert_eq!(DEFAULT_LOG_RETAIN_DAYS, 7);
    }
}
