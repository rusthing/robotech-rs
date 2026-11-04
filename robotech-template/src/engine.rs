//! 模板引擎抽象层与统一调度
//!
//! 本模块提供多引擎并存的基础设施：
//!
//! 1. [`TemplateEngine`] trait：所有引擎的统一接口
//! 2. [`RobotechEngine`]：内置的 robotech-template 引擎（原有逻辑）
//! 3. [`MinijinjaEngine`]：基于 minijinja 的引擎（支持 if/for/switch 等控制流）
//! 4. [`EngineRegistry`]：引擎注册表，按名称查找引擎
//! 5. 统一 [`render`] / [`validate`] 入口：解析首行声明，分派到对应引擎
//!
//! ## 模板格式
//!
//! ```text
//! @engine: minijinja
//! {% if status == "critical" %}
//! 🚨 {{name}}
//! {% else %}
//! ℹ️ {{name}}
//! {% endif %}
//! ```
//!
//! 首行 `@engine: <name>` 声明引擎，后续内容为模板正文。没有声明时默认使用
//! `robotech` 引擎（向后兼容）。

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::diagnostic::{Diagnostic, DiagnosticCode, Severity};
use crate::filter::FilterContext;
use crate::header::{DEFAULT_ENGINE, parse_engine_header};
use crate::render::{RenderOutcome, render_template as robotech_render};
use crate::source::VariableLookup;
use crate::validate::{TemplateCheck, validate_template as robotech_validate};

/// 模板引擎 trait
///
/// 所有引擎实现此接口，由 [`EngineRegistry`] 统一管理。
pub trait TemplateEngine: Send + Sync {
    /// 引擎名称（小写，用于首行声明匹配）
    fn name(&self) -> &str;

    /// 渲染模板
    fn render(
        &self,
        template: &str,
        vars: &dyn VariableLookup,
        ctx: &FilterContext,
    ) -> RenderOutcome;

    /// 校验模板语法（不依赖变量值）
    fn validate(&self, template: &str) -> TemplateCheck;
}

// ─── RobotechEngine ────────────────────────────────────────────────────────────

/// 内置的 robotech-template 引擎
///
/// 使用 `{{key}}` / `${key}` 占位符语法，支持 11 个内置过滤器链。
/// 这是默认引擎，向后兼容所有现有模板。
pub struct RobotechEngine;

impl TemplateEngine for RobotechEngine {
    fn name(&self) -> &str {
        "robotech"
    }

    fn render(
        &self,
        template: &str,
        vars: &dyn VariableLookup,
        ctx: &FilterContext,
    ) -> RenderOutcome {
        robotech_render(template, vars, ctx)
    }

    fn validate(&self, template: &str) -> TemplateCheck {
        robotech_validate(template)
    }
}

// ─── MinijinjaEngine ───────────────────────────────────────────────────────────

/// 基于 minijinja 的模板引擎
///
/// 支持 Jinja2 风格的控制流语法：`{% if %}` / `{% for %}` / `{% switch %}` 等。
/// 变量通过 `{{var}}` 访问，过滤器用 `|` 管道语法。
///
/// ## 内置全局变量
///
/// 引擎自动注入以下变量，模板可直接使用：
///
/// | 变量 | 类型 | 含义 |
/// |---|---|---|
/// | `_time_offset` | i32 | 时区偏移（秒），如 `28800` 表示 `+08:00` |
/// | `_now_ms` | i64 | 渲染时刻的毫秒时间戳 |
///
/// ## 与 robotech 引擎的差异
///
/// - 占位符语法：`{{var}}` 而非 `{{key:过滤器链}}`
/// - 过滤器：使用 minijinja 内置过滤器（`upper`、`lower`、`trim`、`default` 等）
/// - 控制流：支持 `{% if %}` / `{% for %}` / `{% block %}` 等
/// - 缺值语义：未定义变量渲染为空字符串（可通过 `default` 过滤器兜底）
pub struct MinijinjaEngine;

impl TemplateEngine for MinijinjaEngine {
    fn name(&self) -> &str {
        "minijinja"
    }

    fn render(
        &self,
        template: &str,
        vars: &dyn VariableLookup,
        ctx: &FilterContext,
    ) -> RenderOutcome {
        let env = minijinja::Environment::new();

        // 注入全局变量
        let mut globals = HashMap::new();
        globals.insert(
            "_time_offset".to_owned(),
            minijinja::Value::from(ctx.time_offset.local_minus_utc()),
        );
        globals.insert(
            "_now_ms".to_owned(),
            minijinja::Value::from(ctx.now_ms),
        );

        // 收集用户变量
        let user_vars = vars.collect_all();
        for (k, v) in user_vars {
            globals.insert(k, minijinja::Value::from(v));
        }

        // 编译并渲染
        match env.template_from_str(template) {
            Ok(tmpl) => match tmpl.render(&globals) {
                Ok(text) => RenderOutcome {
                    text,
                    diagnostics: Vec::new(),
                },
                Err(err) => RenderOutcome {
                    text: String::new(),
                    diagnostics: vec![Diagnostic::new(
                        Severity::Warning,
                        DiagnosticCode::EngineRenderFailed {
                            engine: "minijinja".to_owned(),
                            reason: err.to_string(),
                        },
                        0,
                        "",
                    )],
                },
            },
            Err(err) => RenderOutcome {
                text: String::new(),
                diagnostics: vec![Diagnostic::new(
                    Severity::Warning,
                    DiagnosticCode::EngineRenderFailed {
                        engine: "minijinja".to_owned(),
                        reason: err.to_string(),
                    },
                    0,
                    "",
                )],
            },
        }
    }

    fn validate(&self, template: &str) -> TemplateCheck {
        let env = minijinja::Environment::new();
        // 尝试编译模板，检查语法错误
        match env.template_from_str(template) {
            Ok(_) => TemplateCheck {
                placeholders: Vec::new(),
                diagnostics: Vec::new(),
            },
            Err(err) => TemplateCheck {
                placeholders: Vec::new(),
                diagnostics: vec![Diagnostic::new(
                    Severity::Error,
                    DiagnosticCode::EngineRenderFailed {
                        engine: "minijinja".to_owned(),
                        reason: err.to_string(),
                    },
                    0,
                    "",
                )],
            },
        }
    }
}

// ─── EngineRegistry ────────────────────────────────────────────────────────────

/// 引擎注册表
///
/// 按名称查找引擎实例。内置 `robotech` 和 `minijinja` 两个引擎。
pub struct EngineRegistry {
    engines: HashMap<String, Box<dyn TemplateEngine>>,
}

impl EngineRegistry {
    /// 创建包含内置引擎的注册表
    pub fn new() -> Self {
        let mut engines: HashMap<String, Box<dyn TemplateEngine>> = HashMap::new();
        engines.insert("robotech".to_owned(), Box::new(RobotechEngine));
        engines.insert("minijinja".to_owned(), Box::new(MinijinjaEngine));
        Self { engines }
    }

    /// 按名称查找引擎
    pub fn get(&self, name: &str) -> Option<&dyn TemplateEngine> {
        self.engines.get(name).map(|e| e.as_ref())
    }

    /// 注册自定义引擎
    pub fn register(&mut self, engine: Box<dyn TemplateEngine>) {
        self.engines.insert(engine.name().to_owned(), engine);
    }

    /// 所有已注册引擎名
    pub fn engine_names(&self) -> Vec<&str> {
        self.engines.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局引擎注册表
static REGISTRY: LazyLock<EngineRegistry> = LazyLock::new(EngineRegistry::new);

// ─── 统一入口 ──────────────────────────────────────────────────────────────────

/// # 统一渲染入口
///
/// 解析模板首行的 `@engine: <name>` 声明，分派到对应引擎渲染。
/// 没有声明时使用默认的 `robotech` 引擎。
///
/// ## 用法
///
/// ```ignore
/// use robotech_template::{render, FilterContext, PriorityVariables};
/// use chrono::{FixedOffset, Utc};
///
/// let template = "@engine: minijinja\n{% if x %}hello {{name}}{% endif %}";
/// let vars = PriorityVariables::new().layer(&HashMap::from([
///     ("x".to_owned(), "1".to_owned()),
///     ("name".to_owned(), "world".to_owned()),
/// ]));
/// let ctx = FilterContext::new(FixedOffset::east_opt(8 * 3600).unwrap(), Utc::now().timestamp_millis());
/// let outcome = render(template, &vars, &ctx);
/// ```
pub fn render(
    template: &str,
    vars: &dyn VariableLookup,
    ctx: &FilterContext,
) -> RenderOutcome {
    let (engine_name, body) = match parse_engine_header(template) {
        Some(header) => (header.engine, header.body),
        None => (DEFAULT_ENGINE.to_owned(), template.to_owned()),
    };

    let Some(engine) = REGISTRY.get(&engine_name) else {
        return RenderOutcome {
            text: String::new(),
            diagnostics: vec![Diagnostic::new(
                Severity::Error,
                DiagnosticCode::UnknownEngine { name: engine_name },
                0,
                "",
            )],
        };
    };

    engine.render(&body, vars, ctx)
}

/// # 统一校验入口
///
/// 解析模板首行的 `@engine: <name>` 声明，分派到对应引擎校验。
/// 没有声明时使用默认的 `robotech` 引擎。
pub fn validate(template: &str) -> TemplateCheck {
    let (engine_name, body) = match parse_engine_header(template) {
        Some(header) => (header.engine, header.body),
        None => (DEFAULT_ENGINE.to_owned(), template.to_owned()),
    };

    let Some(engine) = REGISTRY.get(&engine_name) else {
        return TemplateCheck {
            placeholders: Vec::new(),
            diagnostics: vec![Diagnostic::new(
                Severity::Error,
                DiagnosticCode::UnknownEngine { name: engine_name },
                0,
                "",
            )],
        };
    };

    engine.validate(&body)
}

/// 获取全局引擎注册表的引用
pub fn registry() -> &'static EngineRegistry {
    &REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Utc};

    fn ctx() -> FilterContext {
        FilterContext::new(
            FixedOffset::east_opt(8 * 3600).unwrap(),
            1735689600000, // 2025-01-01 08:00:00 +08:00
        )
    }

    #[test]
    fn robotech_engine_default() {
        let template = "hello {{name}}";
        let vars: HashMap<String, String> = HashMap::from([("name".to_owned(), "world".to_owned())]);
        let outcome = render(template, &vars, &ctx());
        assert_eq!(outcome.text, "hello world");
    }

    #[test]
    fn robotech_engine_explicit() {
        let template = "@engine: robotech\nhello {{name}}";
        let vars: HashMap<String, String> = HashMap::from([("name".to_owned(), "world".to_owned())]);
        let outcome = render(template, &vars, &ctx());
        assert_eq!(outcome.text, "hello world");
    }

    #[test]
    fn minijinja_engine_simple() {
        let template = "@engine: minijinja\nhello {{ name }}";
        let vars: HashMap<String, String> = HashMap::from([("name".to_owned(), "world".to_owned())]);
        let outcome = render(template, &vars, &ctx());
        assert_eq!(outcome.text, "hello world");
    }

    #[test]
    fn minijinja_engine_if() {
        let template = "@engine: minijinja\n{% if show %}visible{% else %}hidden{% endif %}";
        let vars: HashMap<String, String> = HashMap::from([("show".to_owned(), "true".to_owned())]);
        let outcome = render(template, &vars, &ctx());
        assert_eq!(outcome.text, "visible");
    }

    #[test]
    fn minijinja_engine_for() {
        let template = "@engine: minijinja\n{% for i in items %}{{i}} {% endfor %}";
        // minijinja 需要数组类型，这里用 JSON 字符串传入
        // 实际使用中，调用方需要把数组序列化为 minijinja::Value
        let vars: HashMap<String, String> = HashMap::new();
        let outcome = render(template, &vars, &ctx());
        // 没有 items 变量时，for 循环不执行
        assert_eq!(outcome.text, "");
    }

    #[test]
    fn unknown_engine() {
        let template = "@engine: unknown\nbody";
        let vars: HashMap<String, String> = HashMap::new();
        let outcome = render(template, &vars, &ctx());
        assert!(outcome.has_errors());
        assert_eq!(outcome.text, "");
    }

    #[test]
    fn validate_robotech_default() {
        let check = validate("{{name}}");
        assert!(!check.has_errors());
    }

    #[test]
    fn validate_minijinja() {
        let check = validate("@engine: minijinja\n{% if x %}hello{% endif %}");
        assert!(!check.has_errors());
    }

    #[test]
    fn validate_minijinja_syntax_error() {
        let check = validate("@engine: minijinja\n{% if x %}hello");
        assert!(check.has_errors());
    }

    #[test]
    fn validate_unknown_engine() {
        let check = validate("@engine: unknown\nbody");
        assert!(check.has_errors());
    }

    #[test]
    fn minijinja_global_variables() {
        let template = "@engine: minijinja\ntimezone: {{_time_offset}}, now: {{_now_ms}}";
        let vars: HashMap<String, String> = HashMap::new();
        let outcome = render(template, &vars, &ctx());
        assert_eq!(outcome.text, "timezone: 28800, now: 1735689600000");
    }
}
