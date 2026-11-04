//! # 模板渲染（框架层统一入口）
//!
//! 本模块是框架层对 `robotech-template` crate 的封装，为业务层（alarm、msg 等）
//! 提供统一的模板渲染 API。业务层不应直接依赖 `robotech-template`，而应通过本模块
//! 调用。
//!
//! ## 多引擎支持
//!
//! 模板首行可以用 `@engine: <name>` 声明使用哪个引擎：
//!
//! ```text
//! @engine: robotech       使用内置引擎（默认，向后兼容）
//! @engine: minijinja      使用 minijinja 引擎（支持 if/for/switch 等控制流）
//! ```
//!
//! 没有声明时默认使用 `robotech` 引擎。
//!
//! ## 用法
//!
//! ```ignore
//! use robotech::template::{render, validate, FilterContext, PriorityVariables};
//! use chrono::{FixedOffset, Utc};
//!
//! let template = "@engine: minijinja\n{% if x %}hello {{name}}{% endif %}";
//! let vars = PriorityVariables::new().layer(&HashMap::from([
//!     ("x".to_owned(), "1".to_owned()),
//!     ("name".to_owned(), "world".to_owned()),
//! ]));
//! let ctx = FilterContext::new(FixedOffset::east_opt(8 * 3600).unwrap(), Utc::now().timestamp_millis());
//! let outcome = render(template, &vars, &ctx);
//! ```

// 重新导出 robotech-template 的核心类型，业务层通过 robotech::template 访问
pub use robotech_template::{
    // 引擎相关
    EngineRegistry, MinijinjaEngine, RobotechEngine, TemplateEngine,
    // 渲染相关
    FilterContext, FilterFn, PriorityVariables, RenderOutcome, VariableLookup,
    // 校验相关
    PlaceholderInfo, TemplateCheck,
    // 诊断相关
    Diagnostic, DiagnosticCode, Severity,
    // 首行解析
    DEFAULT_ENGINE, parse_engine_header, split_engine_and_body, detect_engine,
    // 过滤器
    filter_names, is_known_filter,
    // 过滤器调用信息
    FilterCallInfo,
};

/// # 渲染模板字符串
///
/// 解析模板首行的 `@engine: <name>` 声明，分派到对应引擎渲染。
/// 没有声明时使用默认的 `robotech` 引擎。
///
/// ## 用法
///
/// ```ignore
/// use robotech::template::{render, FilterContext, PriorityVariables};
/// use std::collections::HashMap;
///
/// let template = "hello {{name}}";
/// let vars = PriorityVariables::new().layer(&HashMap::from([
///     ("name".to_owned(), "world".to_owned()),
/// ]));
/// let ctx = FilterContext::new(FixedOffset::east_opt(8 * 3600).unwrap(), 1735689600000);
/// let outcome = render(template, &vars, &ctx);
/// assert_eq!(outcome.text, "hello world");
/// ```
pub fn render(
    template: &str,
    vars: &dyn VariableLookup,
    ctx: &FilterContext,
) -> RenderOutcome {
    robotech_template::render(template, vars, ctx)
}

/// # 校验模板语法
///
/// 解析模板首行的 `@engine: <name>` 声明，分派到对应引擎校验。
/// 没有声明时使用默认的 `robotech` 引擎。
///
/// 校验不依赖变量值，可在保存模板时调用。
pub fn validate(template: &str) -> TemplateCheck {
    robotech_template::validate(template)
}

/// 获取全局引擎注册表的引用
///
/// 可用于注册自定义引擎：
///
/// ```ignore
/// use robotech::template::registry;
///
/// // 注册自定义引擎
/// registry().register(Box::new(MyEngine));
/// ```
pub fn registry() -> &'static EngineRegistry {
    robotech_template::registry()
}
