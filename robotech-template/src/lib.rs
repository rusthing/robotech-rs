//! # 模板渲染内核
//!
//! 跨服务共用的模板语法与渲染机制。**只提供机制，不内置业务语义**：
//! 变量优先级由调用方通过 [`PriorityVariables`] 逐层传入，时区与「当前时间」
//! 由调用方放进 [`FilterContext`]，本 crate 不读配置、不写日志、不访问数据库。
//!
//! ## 语法
//!
//! ```text
//! {{key}}                        原样代入
//! {{key:过滤器}}                  一个过滤器
//! {{key:过滤器(参数)}}             带参数的过滤器
//! {{key:过滤器(参数)|过滤器}}      用 `|` 串成链，从左到右执行
//! ```
//!
//! `${key}` / `${key:过滤器}` / `${key:过滤器(参数)|过滤器}` 是**完全等价**的
//! 别名，唯一的区别是定界符。`$` 不做环境变量或默认值解释；同一模板里两种
//! 写法可以混用，按出现顺序逐个替换。
//!
//! **旧语法 `{key}` 已废弃，不再支持。** 模板里出现单花括号占位符时不会被替换，
//! 会原样留在输出里；[`validate_template`] 会把它们报成 [`DiagnosticCode::LegacySyntax`]。
//!
//! 模板里的普通花括号（例如 JSON 对象的 `{"k":1}`）不受影响：只有 `{{` 开头、
//! `}}` 结尾（或 `${` 开头、`}` 结尾）且中间是合法变量名/过滤器链的片段才会被
//! 识别为占位符。
//!
//! ## 内置过滤器
//!
//! | 过滤器 | 参数 | 作用 |
//! |---|---|---|
//! | `json` | — | 转成 JSON 字符串字面量（自带引号并转义）；缺值输出 `null` |
//! | `time` | strftime 格式，缺省 `%Y-%m-%d %H:%M:%S` | 时间戳 → 可读时间 |
//! | `since` | — | 时间戳 → 相对时间（`3 分钟前`） |
//! | `upper` / `lower` | — | 大小写转换 |
//! | `trim` | — | 去首尾空白 |
//! | `default` | 兜底文本 | 变量缺失或值为空白时替换成兜底文本 |
//! | `truncate` | 字符数 | 按字符截断，超长补 `…` |
//! | `fixed` | 小数位 | 数字按固定小数位输出，非数字原样 |
//! | `mask` | `首,尾` 位数 | 脱敏：保留首尾若干字符，中间换成 `*` |
//! | `url` | — | percent-encode（空格编成 `%20`） |
//!
//! ## 缺值与失败语义
//!
//! - **变量缺失**：链里没有 `default` / `json` 产出值时，保留原占位符并产生
//!   [`DiagnosticCode::MissingVariable`]；`json` 会把缺值变成 `null`。
//! - **过滤器名写错或语法不对**：整条占位符保留原文，产生
//!   [`DiagnosticCode::UnknownFilter`] / [`DiagnosticCode::InvalidFilterSyntax`]。
//! - **单个过滤器执行失败**（如 `time` 遇到非数字）：跳过它，把当前值交给后面的
//!   过滤器，产生 [`DiagnosticCode::FilterFailed`]。
//!
//! 诊断以结构化结果返回，是否告警、是否拒绝保存由调用方决定。

mod diagnostic;
mod filter;
mod render;
mod source;
mod syntax;
mod validate;

pub use diagnostic::{Diagnostic, DiagnosticCode, Severity};
pub use filter::{FilterContext, FilterFn, filter_names, is_known_filter};
pub use render::{RenderOutcome, render_template};
pub use source::{PriorityVariables, VariableLookup};
pub use validate::{PlaceholderInfo, TemplateCheck, validate_template};

/// 过滤器链里的一次调用（校验结果里回传给调用方）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterCallInfo {
    /// 过滤器名
    pub name: String,
    /// 括号里的参数，原样保留
    pub arg: Option<String>,
}
