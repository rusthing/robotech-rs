//! 渲染：扫描模板、执行过滤器链、收集结构化诊断

use crate::diagnostic::{Diagnostic, DiagnosticCode, Severity};
use crate::filter::{self, FilterContext};
use crate::source::VariableLookup;
use crate::syntax::{VarMatch, find_all, parse_filters};

/// 一次渲染的结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutcome {
    /// 渲染后的文本
    pub text: String,
    /// 渲染过程中发现的问题；不影响已经产出的 `text`
    pub diagnostics: Vec<Diagnostic>,
}

impl RenderOutcome {
    /// 是否有 Error 级诊断（渲染通常只会产生 Warning）
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// # 渲染模板
///
/// 单遍扫描模板：只认 `{{key}}` / `{{key:过滤器链}}`，替换值不会被再次扫描，
/// 因此变量值里出现 `{{...}}` 也不会被二次渲染。
///
/// 变量按 `vars` 的层顺序查找，业务优先级由调用方决定。
pub fn render_template(
    template: &str,
    vars: &dyn VariableLookup,
    ctx: &FilterContext,
) -> RenderOutcome {
    let matches = find_all(template);
    let mut diagnostics = Vec::with_capacity(matches.len());
    let mut text = String::with_capacity(template.len());
    let mut cursor = 0;

    for found in matches {
        text.push_str(&template[cursor..found.offset]);
        text.push_str(&render_placeholder(found, vars, ctx, &mut diagnostics));
        cursor = found.offset + found.whole.len();
    }
    text.push_str(&template[cursor..]);

    RenderOutcome { text, diagnostics }
}

/// 处理单个占位符；失败时保留原文
fn render_placeholder(
    found: VarMatch<'_>,
    vars: &dyn VariableLookup,
    ctx: &FilterContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    let calls = match parse_filters(found.spec) {
        Ok(calls) => calls,
        Err(reason) => {
            diagnostics.push(Diagnostic::new(
                Severity::Warning,
                DiagnosticCode::InvalidFilterSyntax { reason },
                found.offset,
                found.whole,
            ));
            return found.whole.to_owned();
        }
    };

    if let Some(call) = calls.iter().find(|call| !filter::is_known_filter(call.name)) {
        diagnostics.push(Diagnostic::new(
            Severity::Warning,
            DiagnosticCode::UnknownFilter {
                name: call.name.to_owned(),
            },
            found.offset,
            found.whole,
        ));
        return found.whole.to_owned();
    }

    let mut value = vars.lookup(found.key).map(str::to_owned);

    for call in &calls {
        let Some(execute) = filter::lookup(call.name) else {
            // 上面已经拦过未知过滤器，这里只是防御
            continue;
        };
        match execute(value.as_deref(), call.arg, ctx) {
            Ok(next) => value = next,
            Err(reason) => diagnostics.push(Diagnostic::new(
                Severity::Warning,
                DiagnosticCode::FilterFailed {
                    filter: call.name.to_owned(),
                    reason,
                },
                found.offset,
                found.whole,
            )),
        }
    }

    match value {
        Some(value) => value,
        None => {
            diagnostics.push(Diagnostic::new(
                Severity::Warning,
                DiagnosticCode::MissingVariable {
                    key: found.key.to_owned(),
                },
                found.offset,
                found.whole,
            ));
            found.whole.to_owned()
        }
    }
}
