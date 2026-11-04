//! 模板首行引擎声明解析
//!
//! 模板字段的首行可以是 `@engine: <name>` 形式的引擎声明，用于指定使用哪个
//! 模板引擎渲染后续内容。没有声明时默认使用 `robotech` 引擎（向后兼容）。
//!
//! ## 语法
//!
//! ```text
//! @engine: robotech       使用内置的 robotech-template 引擎
//! @engine: minijinja      使用 minijinja 引擎（支持 if/for/switch 等控制流）
//! ```
//!
//! 声明行必须是模板的第一行，以 `@engine:` 开头（允许前导空白），后面跟引擎名。
//! 声明行之后的所有内容（包括空行）都是模板正文。
//!
//! ## 设计约束
//!
//! - 声明语法使用 `@engine:` 前缀，不与任何引擎的模板语法冲突
//!   （robotech 用 `{{...}}` / `${...}`，minijinja 用 `{%...%}` / `{{...}}`）
//! - 没有声明行的模板默认走 `robotech` 引擎，保证向后兼容
//! - 引擎名不区分大小写（`MiniJinja` / `minijinja` 等价）

/// 引擎声明的解析结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineHeader {
    /// 引擎名（小写规范化）
    pub engine: String,
    /// 模板正文（声明行之后的内容）
    pub body: String,
}

/// 默认引擎名
pub const DEFAULT_ENGINE: &str = "robotech";

/// # 解析模板首行的引擎声明
///
/// 如果首行是 `@engine: <name>` 形式，返回 `Some(EngineHeader)`；
/// 否则返回 `None`，表示使用默认引擎（`robotech`）。
///
/// ## 用法
///
/// ```ignore
/// let template = "@engine: minijinja\n{% if x %}hello{% endif %}";
/// let header = parse_engine_header(template);
/// assert_eq!(header.unwrap().engine, "minijinja");
/// ```
pub fn parse_engine_header(template: &str) -> Option<EngineHeader> {
    // 找到第一行的结尾
    let line_end = template.find('\n').unwrap_or(template.len());
    let first_line = template[..line_end].trim();

    // 检查是否以 @engine: 开头（允许前导空白，上面已经 trim 了）
    let rest = first_line.strip_prefix("@engine:")?;
    let engine = rest.trim();

    // 引擎名不能为空
    if engine.is_empty() {
        return None;
    }

    // 规范化为小写
    let engine = engine.to_lowercase();

    // 提取正文：声明行之后的内容
    let body = if line_end < template.len() {
        // 跳过换行符
        template[line_end + 1..].to_owned()
    } else {
        // 模板只有声明行，没有正文
        String::new()
    };

    Some(EngineHeader { engine, body })
}

/// # 获取模板使用的引擎名
///
/// 解析首行声明，有声明则返回声明的引擎名，否则返回默认引擎名 `robotech`。
pub fn detect_engine(template: &str) -> String {
    parse_engine_header(template)
        .map(|h| h.engine)
        .unwrap_or_else(|| DEFAULT_ENGINE.to_owned())
}

/// # 拆分模板为引擎名和正文
///
/// 返回 `(engine_name, body)`。没有声明时引擎名为 `robotech`，正文为原模板。
pub fn split_engine_and_body(template: &str) -> (String, String) {
    match parse_engine_header(template) {
        Some(header) => (header.engine, header.body),
        None => (DEFAULT_ENGINE.to_owned(), template.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minijinja_header() {
        let template = "@engine: minijinja\n{% if x %}hello{% endif %}";
        let header = parse_engine_header(template).unwrap();
        assert_eq!(header.engine, "minijinja");
        assert_eq!(header.body, "{% if x %}hello{% endif %}");
    }

    #[test]
    fn parses_robotech_header() {
        let template = "@engine: robotech\n{{name}}";
        let header = parse_engine_header(template).unwrap();
        assert_eq!(header.engine, "robotech");
        assert_eq!(header.body, "{{name}}");
    }

    #[test]
    fn case_insensitive() {
        let template = "@engine: MiniJinja\nbody";
        let header = parse_engine_header(template).unwrap();
        assert_eq!(header.engine, "minijinja");
    }

    #[test]
    fn allows_leading_whitespace() {
        let template = "  @engine: minijinja\nbody";
        let header = parse_engine_header(template).unwrap();
        assert_eq!(header.engine, "minijinja");
    }

    #[test]
    fn no_header_returns_none() {
        assert!(parse_engine_header("{{name}}").is_none());
        assert!(parse_engine_header("plain text").is_none());
        assert!(parse_engine_header("").is_none());
    }

    #[test]
    fn empty_engine_name_returns_none() {
        assert!(parse_engine_header("@engine:\nbody").is_none());
        assert!(parse_engine_header("@engine:  \nbody").is_none());
    }

    #[test]
    fn header_only_no_body() {
        let template = "@engine: minijinja";
        let header = parse_engine_header(template).unwrap();
        assert_eq!(header.engine, "minijinja");
        assert_eq!(header.body, "");
    }

    #[test]
    fn multiline_body_preserved() {
        let template = "@engine: minijinja\nline1\nline2\nline3";
        let header = parse_engine_header(template).unwrap();
        assert_eq!(header.body, "line1\nline2\nline3");
    }

    #[test]
    fn detect_engine_default() {
        assert_eq!(detect_engine("{{name}}"), "robotech");
        assert_eq!(detect_engine("@engine: minijinja\nbody"), "minijinja");
    }

    #[test]
    fn split_engine_and_body_default() {
        let (engine, body) = split_engine_and_body("{{name}}");
        assert_eq!(engine, "robotech");
        assert_eq!(body, "{{name}}");
    }

    #[test]
    fn split_engine_and_body_explicit() {
        let (engine, body) = split_engine_and_body("@engine: minijinja\n{% if x %}hi{% endif %}");
        assert_eq!(engine, "minijinja");
        assert_eq!(body, "{% if x %}hi{% endif %}");
    }
}
