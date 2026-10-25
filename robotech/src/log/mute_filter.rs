//! # 日志屏蔽过滤模块
//!
//! 按 target 与内容关键字屏蔽日志事件：target 命中且文本命中关键字的事件被丢弃，
//! 其余日志照常输出。

use arc_swap::ArcSwap;
use std::sync::Arc;
use tracing_core::field::{Field, Visit};
use tracing_core::{Event, Metadata, Subscriber};
use tracing_subscriber::layer::{Context, Filter};

/// # 日志屏蔽过滤器
///
/// 一个 per-layer 过滤器（[`Filter`]），按两级条件判断是否丢弃一条日志：
///
/// 1. **target**：事件 target 以配置项之一为前缀才继续判断；`targets` 为空表示不限制 target。
///    因此 `sqlx`、`sqlx::query` 都能匹配 `sqlx::query` 事件。
/// 2. **关键字**：事件的所有字段值（`message`、`db.statement`、`summary`、任意 kv）拼成一段文本，
///    与关键字做**大小写不敏感**的子串匹配，命中任意一个即丢弃。
///
/// 关键字是纯子串比较，不需要转义：`"dtw_locator_bind"`、`"FROM \"a\".\"b\""`、
/// `"WHERE a = 1 AND b = 2"` 这类带双引号、空格、括号的内容都能直接配。
///
/// 两个列表都存在 `Arc<ArcSwap<Vec<String>>>` 中，日志配置热更新时整体替换内容即可，
/// 无需重建 subscriber。
///
/// ## 示例
/// ```ignore
/// let filter = MuteFilter::new(
///     Arc::new(ArcSwap::from_pointee(vec!["sqlx::query".to_string()])),
///     Arc::new(ArcSwap::from_pointee(vec!["dtw_locator_bind".to_string()])),
/// );
/// tracing_subscriber::registry()
///     .with(fmt::layer().with_filter(filter))
///     .init();
/// ```
#[derive(Clone)]
pub struct MuteFilter {
    /// 参与匹配的 target 前缀列表（空 = 不限制 target），由日志配置热更新
    targets: Arc<ArcSwap<Vec<String>>>,
    /// 屏蔽关键字（**必须已转小写**，命中即丢弃），由日志配置热更新
    keywords: Arc<ArcSwap<Vec<String>>>,
}

impl MuteFilter {
    /// # 创建日志屏蔽过滤器
    ///
    /// ## 参数
    /// * `targets` - 共享的 target 前缀列表句柄，为空表示对所有 target 生效
    /// * `keywords` - 共享的关键字列表句柄，调用方需保证已转小写
    ///
    /// ## 返回值
    /// 返回 `MuteFilter` 实例
    pub fn new(targets: Arc<ArcSwap<Vec<String>>>, keywords: Arc<ArcSwap<Vec<String>>>) -> Self {
        Self { targets, keywords }
    }
}

impl<S> Filter<S> for MuteFilter
where
    S: Subscriber,
{
    /// 事件级别拿不到字段值，这里只做放行，真正的判断在 `event_enabled`
    ///
    /// ## 参数
    /// * `_metadata` - 事件/span 元信息
    /// * `_cx` - 当前订阅器上下文
    ///
    /// ## 返回值
    /// 恒返回 `true`
    fn enabled(&self, _metadata: &Metadata<'_>, _cx: &Context<'_, S>) -> bool {
        true
    }

    /// 按 target 前缀与内容关键字判断是否丢弃该事件
    ///
    /// ## 参数
    /// * `event` - 待判断的日志事件
    /// * `_cx` - 当前订阅器上下文
    ///
    /// ## 返回值
    /// 命中屏蔽规则返回 `false`（丢弃该条日志），否则返回 `true`
    fn event_enabled(&self, event: &Event<'_>, _cx: &Context<'_, S>) -> bool {
        let targets = self.targets.load();
        if !targets.is_empty() {
            let target = event.metadata().target();
            let hit = targets
                .iter()
                .any(|prefix| !prefix.is_empty() && target.starts_with(prefix.as_str()));
            if !hit {
                return true;
            }
        }

        let keywords = self.keywords.load();
        if keywords.is_empty() {
            return true;
        }

        let mut visitor = TextVisitor::default();
        event.record(&mut visitor);
        let text = visitor.text.to_lowercase();

        !keywords
            .iter()
            .any(|keyword| !keyword.is_empty() && text.contains(keyword.as_str()))
    }
}

/// 事件文本收集访问器
///
/// 收集事件全部字段值（不区分字段名），拼成一段文本供关键字匹配。
#[derive(Default)]
struct TextVisitor {
    /// 拼接后的事件文本
    text: String,
}

impl TextVisitor {
    /// # 追加字段值
    ///
    /// ## 参数
    /// * `value` - 字段值
    ///
    /// ## 返回值
    /// 无
    fn push(&mut self, value: &str) {
        if !self.text.is_empty() {
            self.text.push(' ');
        }
        self.text.push_str(value);
    }
}

impl Visit for TextVisitor {
    /// 记录字符串字段
    ///
    /// ## 参数
    /// * `_field` - 字段元信息
    /// * `value` - 字段值
    ///
    /// ## 返回值
    /// 无
    fn record_str(&mut self, _field: &Field, value: &str) {
        self.push(value);
    }

    /// 记录以 Debug 形式提供的字段（`message` 等通常走这里）
    ///
    /// ## 参数
    /// * `_field` - 字段元信息
    /// * `value` - 字段值
    ///
    /// ## 返回值
    /// 无
    fn record_debug(&mut self, _field: &Field, value: &dyn std::fmt::Debug) {
        self.push(&format!("{value:?}"));
    }
}
