//! 模板校验：括号配对、旧语法、过滤器名与过滤器语法
//!
//! 校验不依赖变量值，可在保存模板时调用。它返回 Error 级诊断，调用方据此
//! 拒绝保存；渲染期的 Warning 不在这里重复。

use crate::diagnostic::{Diagnostic, DiagnosticCode, Severity};
use crate::filter;
use crate::syntax::{find_all, parse_filters};

use crate::FilterCallInfo;
use regex::Regex;
use std::sync::LazyLock;

/// 旧单花括号占位符（已废弃）
static LEGACY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{(\w+)(?::([^{}]*))?\}").expect("旧语法正则表达式编译失败")
});

/// 一个合法占位符的解析结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceholderInfo {
    /// 整段占位符（含 `{{` / `}}`）
    pub whole: String,
    /// 变量名
    pub key: String,
    /// `:` 之后的过滤器链原文
    pub spec: String,
    /// 解析出的过滤器调用；语法错误时为空
    pub filters: Vec<FilterCallInfo>,
    /// 在模板中的字节偏移
    pub offset: usize,
}

/// 校验结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateCheck {
    /// 识别到的合法占位符
    pub placeholders: Vec<PlaceholderInfo>,
    /// 问题清单
    pub diagnostics: Vec<Diagnostic>,
}

impl TemplateCheck {
    /// 是否存在 Error 级问题（存在时建议拒绝保存）
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// # 校验模板
///
/// 检查项：
/// 1. `{{` / `${` 是否闭合；
/// 2. 是否残留旧语法 `{key}`；
/// 3. 过滤器链语法是否正确；
/// 4. 过滤器名是否已注册。
///
/// `{{key}}` 与 `${key}` 是等价写法，都算合法占位符；普通花括号（JSON 对象、
/// 正文里的 `{`）不会报错。
pub fn validate_template(template: &str) -> TemplateCheck {
    let matches = find_all(template);
    let mut diagnostics = Vec::new();
    let mut placeholders = Vec::with_capacity(matches.len());
    let mut cursor = 0;

    for found in &matches {
        diagnose_gap(
            &template[cursor..found.offset],
            cursor,
            &mut diagnostics,
        );

        match parse_filters(found.spec) {
            Ok(calls) => {
                for call in &calls {
                    if !filter::is_known_filter(call.name) {
                        diagnostics.push(Diagnostic::new(
                            Severity::Error,
                            DiagnosticCode::UnknownFilter {
                                name: call.name.to_owned(),
                            },
                            found.offset,
                            found.whole,
                        ));
                    }
                }
                placeholders.push(PlaceholderInfo {
                    whole: found.whole.to_owned(),
                    key: found.key.to_owned(),
                    spec: found.spec.to_owned(),
                    filters: calls
                        .iter()
                        .map(|call| FilterCallInfo {
                            name: call.name.to_owned(),
                            arg: call.arg.map(str::to_owned),
                        })
                        .collect(),
                    offset: found.offset,
                });
            }
            Err(reason) => {
                diagnostics.push(Diagnostic::new(
                    Severity::Error,
                    DiagnosticCode::InvalidFilterSyntax { reason },
                    found.offset,
                    found.whole,
                ));
                placeholders.push(PlaceholderInfo {
                    whole: found.whole.to_owned(),
                    key: found.key.to_owned(),
                    spec: found.spec.to_owned(),
                    filters: Vec::new(),
                    offset: found.offset,
                });
            }
        }

        cursor = found.offset + found.whole.len();
    }

    diagnose_gap(&template[cursor..], cursor, &mut diagnostics);

    TemplateCheck {
        placeholders,
        diagnostics,
    }
}

/// 检查两个合法占位符之间的普通文本：未闭合的 `{{` / `${` 与旧语法
fn diagnose_gap(segment: &str, base: usize, diagnostics: &mut Vec<Diagnostic>) {
    let mut cursor = 0;

    while let Some(rel) = segment[cursor..].find('{') {
        let start = cursor + rel;
        let rest = &segment[start..];

        if let Some(after) = rest.strip_prefix("{{") {
            match after.find("}}") {
                Some(close) => {
                    // 括号闭合，内容是否合法由 find_all 负责；跳到闭合之后避免
                    // 把内层当成旧语法
                    cursor = start + 2 + close + 2;
                    continue;
                }
                None => {
                    diagnostics.push(Diagnostic::new(
                        Severity::Error,
                        DiagnosticCode::UnclosedPlaceholder,
                        base + start,
                        snippet(rest),
                    ));
                    return;
                }
            }
        }

        // `${...}` 里合法的部分已经被 find_all 消费；走到这里的要么是未闭合，
        // 要么内容不是合法变量名/过滤器链。后者按普通文本跳过，不能误报成旧语法。
        if start > 0 && segment.as_bytes()[start - 1] == b'$' {
            let dollar = start - 1;
            match rest[1..].find('}') {
                Some(close) => {
                    cursor = start + 1 + close + 1;
                    continue;
                }
                None => {
                    diagnostics.push(Diagnostic::new(
                        Severity::Error,
                        DiagnosticCode::UnclosedPlaceholder,
                        base + dollar,
                        snippet(&segment[dollar..]),
                    ));
                    return;
                }
            }
        }

        // 单花括号：只有形如 `{key}` / `{key:...}` 才按旧语法报错，
        // JSON 的 `{"k":...}` 因为 `{` 后不是 `\w` 不受影响
        if let Some(found) = LEGACY_RE.find_at(segment, start)
            && found.start() == start
        {
            diagnostics.push(Diagnostic::new(
                Severity::Error,
                DiagnosticCode::LegacySyntax,
                base + start,
                found.as_str(),
            ));
            cursor = found.end();
            continue;
        }

        cursor = start + '{'.len_utf8();
    }
}

/// 截取片段用于诊断展示
fn snippet(text: &str) -> String {
    const MAX: usize = 32;
    let snippet: String = text.chars().take(MAX).collect();
    if text.chars().count() > MAX {
        format!("{snippet}…")
    } else {
        snippet
    }
}
