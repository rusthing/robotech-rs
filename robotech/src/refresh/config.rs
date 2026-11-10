//! # 刷新信号配置
//!
//! 配置文件中的 `[refresh]` 段（历史名称 `[local-cache]` 仍兼容）：
//!
//! ```toml
//! [refresh]
//! # 订阅探活间隔，支持 humantime 写法（15s / 1m / 500ms）
//! probe-interval = "15s"
//! # 兜底全量重建间隔：即使没收到任何刷新信号，也按这个间隔执行一次动作。
//! # 0s 表示关闭兜底（只靠信号驱动）。
//! rebuild-interval = "60s"
//! ```
//!
//! 配置在应用配置加载与热更新时由 [`load_config`] 写入全局，
//! 订阅守护任务每轮读取一次，因此改配置后下一轮探活即生效，无需重启进程。
//!
//! 未配置该段时使用默认值（探活 15 秒、兜底重建 60 秒）。

use arc_swap::ArcSwapOption;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;
use wheel_rs::serde::duration_serde;

/// 刷新配置在配置文件中的段名
pub const REFRESH_CONFIG_KEY: &str = "refresh";

/// 历史段名（本地缓存时期使用），仍兼容，`[refresh]` 优先
pub const LEGACY_LOCAL_CACHE_CONFIG_KEY: &str = "local-cache";

/// 默认探活间隔
const DEFAULT_PROBE_INTERVAL: Duration = Duration::from_secs(15);

/// 默认兜底全量重建间隔
///
/// 刷新信号允许丢，但「发布侧漏发」「信号被别的 Stream 吞掉」「写入根本没
/// 走通知」这类问题不会触发 consumer 重建，探活看到的仍是一切正常。周期性
/// 执行一次全量重建，把这类静默不一致限制在一个间隔内。
const DEFAULT_REBUILD_INTERVAL: Duration = Duration::from_secs(60);

/// 探活间隔下限
///
/// 配置成 0 会让守护任务空转，这里统一按下限处理。
const MIN_PROBE_INTERVAL: Duration = Duration::from_secs(1);

/// # 刷新信号配置
///
/// 对应配置文件中的 `[refresh]` 段，字段缺省时取 [`Default`] 的值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct RefreshConfig {
    /// 订阅守护的探活间隔
    ///
    /// 守护任务每隔这个间隔探测一次 `consumer` 是否还在；丢失则重建订阅
    /// 并立即执行一次刷新动作。
    #[serde(with = "duration_serde")]
    pub probe_interval: Duration,
    /// 兜底全量重建间隔
    ///
    /// 订阅正常但始终没收到刷新信号时，也按这个间隔执行一次动作，防止缓存
    /// 长期停在旧快照。`0s` 关闭兜底，只靠信号驱动。
    #[serde(with = "duration_serde")]
    pub rebuild_interval: Duration,
}

impl Default for RefreshConfig {
    fn default() -> Self {
        Self {
            probe_interval: DEFAULT_PROBE_INTERVAL,
            rebuild_interval: DEFAULT_REBUILD_INTERVAL,
        }
    }
}

impl RefreshConfig {
    /// 实际生效的探活间隔（低于下限时按下限返回）
    pub fn effective_probe_interval(&self) -> Duration {
        self.probe_interval.max(MIN_PROBE_INTERVAL)
    }

    /// 实际生效的兜底重建间隔
    ///
    /// `0s` 表示关闭；其余值与探活间隔共用下限，避免配成 1ms 让守护任务
    /// 反复全量查库。
    pub fn effective_rebuild_interval(&self) -> Option<Duration> {
        if self.rebuild_interval.is_zero() {
            None
        } else {
            Some(self.rebuild_interval.max(MIN_PROBE_INTERVAL))
        }
    }
}

/// 全局刷新配置
///
/// 由配置加载流程写入，订阅守护任务读取。
static REFRESH_CONFIG: ArcSwapOption<RefreshConfig> = ArcSwapOption::const_empty();

/// 设置刷新配置
///
/// 配置加载与热更新时由 [`load_config`] 调用；业务代码也可直接
/// 调用以覆盖配置文件中的取值。
pub fn set_config(config: RefreshConfig) {
    REFRESH_CONFIG.store(Some(Arc::new(config)));
}

/// 获取当前刷新配置
///
/// 尚未加载配置（如单元测试）时返回默认值。
pub fn get_refresh_config() -> Arc<RefreshConfig> {
    REFRESH_CONFIG
        .load_full()
        .unwrap_or_else(|| Arc::new(RefreshConfig::default()))
}

/// # 从完整配置对象中加载刷新配置
///
/// 供应用配置加载与热更新路径调用：
///
/// - 段不存在：沿用当前值（首次启动即默认值），不覆盖已生效的配置；
/// - 段存在但解析失败：打 warn 并沿用当前值，不阻断启动；
/// - 解析成功：写入全局，下一轮探活生效。
///
/// `[refresh]` 优先，未配置时回退到历史段名 `[local-cache]`。
// `refresh` 模块常驻（`RefreshSubject` 要给 api crate 用），但只有同时启用
// `app` 与 `refresh` 时才会被 `build_app_cfg` 调用。
#[cfg_attr(not(all(feature = "app", feature = "refresh")), allow(dead_code))]
pub(crate) fn load_config(config: &::config::Config) {
    for key in [REFRESH_CONFIG_KEY, LEGACY_LOCAL_CACHE_CONFIG_KEY] {
        if config.get_table(key).is_err() {
            continue;
        }

        match config.get::<RefreshConfig>(key) {
            Ok(refresh_config) => {
                let configured = refresh_config.probe_interval;
                if configured < MIN_PROBE_INTERVAL {
                    warn!(
                        "{key}.probe-interval={configured:?} 低于下限，按 {MIN_PROBE_INTERVAL:?} 处理"
                    );
                }
                set_config(refresh_config);
            }
            Err(e) => warn!("{key} 配置解析失败，沿用当前值: {e}"),
        }

        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml_str: &str) -> RefreshConfig {
        ::config::Config::builder()
            .add_source(::config::File::from_str(
                toml_str,
                ::config::FileFormat::Toml,
            ))
            .build()
            .unwrap()
            .get(REFRESH_CONFIG_KEY)
            .unwrap()
    }

    #[test]
    fn default_probe_interval_is_15s() {
        assert_eq!(
            RefreshConfig::default().probe_interval,
            Duration::from_secs(15)
        );
        // 只配段名、不配字段时取默认值
        assert_eq!(parse("[refresh]").probe_interval, Duration::from_secs(15));
    }

    #[test]
    fn probe_interval_accepts_humantime() {
        assert_eq!(
            parse("[refresh]\nprobe-interval = \"1m\"\n").probe_interval,
            Duration::from_secs(60)
        );
        assert_eq!(
            parse("[refresh]\nprobe-interval = \"500ms\"\n").probe_interval,
            Duration::from_millis(500)
        );
    }

    #[test]
    fn probe_interval_has_lower_bound() {
        let cfg = parse("[refresh]\nprobe-interval = \"0s\"\n");
        assert_eq!(cfg.effective_probe_interval(), MIN_PROBE_INTERVAL);
    }

    #[test]
    fn rebuild_interval_defaults_to_60s() {
        assert_eq!(
            RefreshConfig::default().rebuild_interval,
            Duration::from_secs(60)
        );
        assert_eq!(parse("[refresh]").rebuild_interval, Duration::from_secs(60));
    }

    #[test]
    fn rebuild_interval_can_be_disabled_or_clamped() {
        let disabled = parse("[refresh]\nrebuild-interval = \"0s\"\n");
        assert_eq!(disabled.effective_rebuild_interval(), None);

        let clamped = parse("[refresh]\nrebuild-interval = \"200ms\"\n");
        assert_eq!(
            clamped.effective_rebuild_interval(),
            Some(MIN_PROBE_INTERVAL)
        );

        let configured = parse("[refresh]\nrebuild-interval = \"5m\"\n");
        assert_eq!(
            configured.effective_rebuild_interval(),
            Some(Duration::from_secs(300))
        );
    }
}
