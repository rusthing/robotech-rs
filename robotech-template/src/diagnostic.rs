//! 结构化诊断
//!
//! 渲染与校验都不直接打日志：本 crate 不认识调用方的日志设施，也不该替调用方
//! 决定严重级别。调用方拿 [`Diagnostic`] 自行决定 warn / error / 拒绝保存。

use std::fmt;

/// 诊断的严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// 渲染仍能产出结果，但模板或变量有问题
    Warning,
    /// 模板本身不合法（校验时使用）
    Error,
}

/// 诊断原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticCode {
    /// `{{` 没有对应的 `}}`
    UnclosedPlaceholder,
    /// 出现了单花括号占位符（旧语法已废弃）
    LegacySyntax,
    /// 过滤器链语法错误
    InvalidFilterSyntax { reason: String },
    /// 过滤器名不在注册表里
    UnknownFilter { name: String },
    /// 渲染时变量缺失（且没有过滤器兜住）
    MissingVariable { key: String },
    /// 某个过滤器执行失败，已跳过
    FilterFailed { filter: String, reason: String },
}

/// 一条诊断
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 严重级别
    pub severity: Severity,
    /// 原因
    pub code: DiagnosticCode,
    /// 占位符在模板中的字节偏移
    pub offset: usize,
    /// 原占位符文本（未闭合时为片段）
    pub placeholder: String,
}

impl Diagnostic {
    pub(crate) fn new(
        severity: Severity,
        code: DiagnosticCode,
        offset: usize,
        placeholder: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code,
            offset,
            placeholder: placeholder.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.code {
            DiagnosticCode::UnclosedPlaceholder => write!(
                f,
                "模板占位符未闭合（偏移 {}）：{}",
                self.offset, self.placeholder
            ),
            DiagnosticCode::LegacySyntax => write!(
                f,
                "模板占位符 {} 使用了已废弃的单花括号语法（偏移 {}），应写成 {{{{key}}}}",
                self.placeholder, self.offset
            ),
            DiagnosticCode::InvalidFilterSyntax { reason } => write!(
                f,
                "模板变量 {} 语法错误（{reason}），保留占位符",
                self.placeholder
            ),
            DiagnosticCode::UnknownFilter { name } => write!(
                f,
                "模板变量 {} 含未知过滤器 {name}，保留占位符",
                self.placeholder
            ),
            DiagnosticCode::MissingVariable { key } => {
                write!(f, "模板变量未提供: {{{{{key}}}}}，保留占位符")
            }
            DiagnosticCode::FilterFailed { filter, reason } => write!(
                f,
                "模板变量 {} 的过滤器 {filter} 处理失败：{reason}，跳过",
                self.placeholder
            ),
        }
    }
}
