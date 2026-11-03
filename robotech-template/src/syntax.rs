//! 语法解析：`{{key}}` / `{{key:过滤器链}}`，以及等价别名 `${key}`

use regex::Regex;
use std::sync::LazyLock;

/// 匹配 `{{key}}` 与 `{{key:<过滤器链>}}`
///
/// 过滤器链里不允许出现花括号，参数放在括号里，可以包含 `|`、`:`、空格。
static VAR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{(\w+)(?::([^{}]*))?\}\}").expect("模板正则表达式编译失败")
});

/// 匹配 `${key}` 与 `${key:<过滤器链>}`（与 `{{...}}` 完全等价）
///
/// 不做任何环境变量/默认值解释，`$` 只是占位符前缀的一部分。
static ALIAS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{(\w+)(?::([^{}]*))?\}").expect("别名正则表达式编译失败")
});

/// 模板里的一次占位符匹配
#[derive(Debug, Clone, Copy)]
pub(crate) struct VarMatch<'a> {
    /// 整段占位符（含定界符：`{{` `}}` 或 `${` `}`）
    pub whole: &'a str,
    /// 变量名
    pub key: &'a str,
    /// `:` 之后、右侧定界符之前的过滤器链，空串表示没有过滤器
    pub spec: &'a str,
    /// 在模板中的字节偏移
    pub offset: usize,
}

/// 过滤器链里的一次调用
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FilterCall<'a> {
    pub name: &'a str,
    pub arg: Option<&'a str>,
}

/// 从 `from` 开始找下一个占位符
///
/// `{{x}}` 与 `${x}` 两种写法都算，取两者中更靠前的一个；同一模板混写时
/// 按出现顺序逐个替换（同位置不可能同时命中，前缀不同）。
pub(crate) fn next_match<'a>(text: &'a str, from: usize) -> Option<VarMatch<'a>> {
    let braces = VAR_RE.captures_at(text, from).and_then(|caps| build_match(&caps));
    let alias = ALIAS_RE.captures_at(text, from).and_then(|caps| build_match(&caps));
    match (braces, alias) {
        (Some(a), Some(b)) => Some(if a.offset <= b.offset { a } else { b }),
        (Some(found), None) | (None, Some(found)) => Some(found),
        (None, None) => None,
    }
}

/// 从正则捕获构造 [`VarMatch`]
fn build_match<'a>(caps: &regex::Captures<'a>) -> Option<VarMatch<'a>> {
    let whole = caps.get(0)?;
    Some(VarMatch {
        whole: whole.as_str(),
        key: caps.get(1)?.as_str(),
        spec: caps.get(2).map_or("", |m| m.as_str()),
        offset: whole.start(),
    })
}

/// 收集模板里的全部占位符
pub(crate) fn find_all(text: &str) -> Vec<VarMatch<'_>> {
    let mut matches = Vec::new();
    let mut cursor = 0;
    while let Some(found) = next_match(text, cursor) {
        cursor = found.offset + found.whole.len();
        matches.push(found);
    }
    matches
}

/// # 解析过滤器链
///
/// 语法：`名字` 或 `名字(参数)`，多项用 `|` 连接。手写扫描而不是 `split('|')`
/// ——参数在括号里，可以包含 `|`，全局切分会把参数切断。
///
/// 参数到**第一个** `)` 为止；需要 `)` 本身时目前无法表达，内置过滤器都用不上。
pub(crate) fn parse_filters(spec: &str) -> Result<Vec<FilterCall<'_>>, String> {
    let mut calls = Vec::new();
    let bytes = spec.as_bytes();
    let mut cursor = 0;

    loop {
        // 允许 `a | b` 这种写法，分隔符周围留空不影响
        while cursor < bytes.len() && (bytes[cursor] == b'|' || bytes[cursor].is_ascii_whitespace())
        {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }

        let name_start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric()
                || bytes[cursor] == b'_'
                || bytes[cursor] == b'-')
        {
            cursor += 1;
        }
        if name_start == cursor {
            return Err(format!("{} 处缺少过滤器名", &spec[name_start..]));
        }
        let name = &spec[name_start..cursor];

        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }

        let arg = if cursor < bytes.len() && bytes[cursor] == b'(' {
            let arg_start = cursor + 1;
            let Some(close) = spec[arg_start..].find(')') else {
                return Err(format!("{name} 的参数缺少右括号"));
            };
            cursor = arg_start + close + 1;
            Some(&spec[arg_start..cursor - 1])
        } else {
            None
        };

        calls.push(FilterCall { name, arg });

        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] != b'|' {
            return Err(format!(
                "{name} 之后有多余字符：{}",
                spec[cursor..].trim_end_matches('}')
            ));
        }
    }

    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_only_double_brace_placeholders() {
        let found = find_all(r#"{"a":{{x}},"b":{{y:upper}}}"#);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].key, "x");
        assert_eq!(found[0].spec, "");
        assert_eq!(found[1].key, "y");
        assert_eq!(found[1].spec, "upper");
    }

    #[test]
    fn single_braces_are_not_placeholders() {
        assert!(find_all(r#"{"a":1,"b":{x}}"#).is_empty());
        assert!(find_all("系统通知").is_empty());
    }

    #[test]
    fn finds_alias_placeholders() {
        let found = find_all("${x} 与 ${y:upper}");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].key, "x");
        assert_eq!(found[0].whole, "${x}");
        assert_eq!(found[0].spec, "");
        assert_eq!(found[1].key, "y");
        assert_eq!(found[1].spec, "upper");
    }

    #[test]
    fn mixed_forms_keep_occurrence_order() {
        let found = find_all("{{a}}/${b}/{{c}}/${d:json}");
        let keys: Vec<_> = found.iter().map(|found| found.key).collect();
        assert_eq!(keys, ["a", "b", "c", "d"]);
        assert_eq!(found[0].offset, 0);
        assert_eq!(found[1].offset, 6);
    }

    #[test]
    fn parses_pipe_inside_parens() {
        let calls = parse_filters("default(a|b)|json").unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].arg, Some("a|b"));
        assert_eq!(calls[1].name, "json");
    }
}
