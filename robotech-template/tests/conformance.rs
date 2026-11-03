//! 新语法一致性测试：覆盖全部过滤器、缺值与边角语义、校验诊断

use chrono::FixedOffset;
use robotech_template::{
    DiagnosticCode, FilterContext, PriorityVariables, Severity, VariableLookup, filter_names,
    is_known_filter, render_template, validate_template,
};
use std::collections::HashMap;

fn ctx() -> FilterContext {
    FilterContext::new(
        FixedOffset::east_opt(8 * 3600).expect("合法时区"),
        1_735_689_700_000, // 2025-01-01 08:01:40 +08:00
    )
}

fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

fn render_with(template: &str, vars: &dyn VariableLookup) -> String {
    render_template(template, vars, &ctx()).text
}

fn render(template: &str, pairs: &[(&str, &str)]) -> String {
    render_with(template, &vars(pairs))
}

fn render_with_builtins(
    template: &str,
    pairs: &[(&str, &str)],
    builtins: &[(&'static str, &str)],
) -> String {
    let builtins: HashMap<&'static str, String> = builtins
        .iter()
        .map(|(k, v)| (*k, (*v).to_string()))
        .collect();
    let labels = vars(pairs);
    let annotations: HashMap<String, String> = HashMap::new();
    let all = PriorityVariables::new()
        .layer(&builtins)
        .layer(&labels)
        .layer(&annotations);
    render_with(template, &all)
}

#[test]
fn all_variables_render() {
    let result = render(
        "你好 {{name}}，你的订单 {{order_id}} 已发货",
        &[("name", "张三"), ("order_id", "ORD-001")],
    );
    assert_eq!(result, "你好 张三，你的订单 ORD-001 已发货");
}

#[test]
fn missing_variable_keeps_placeholder() {
    let result = render("你好 {{name}}，你的订单 {{order_id}} 已发货", &[]);
    assert_eq!(result, "你好 {{name}}，你的订单 {{order_id}} 已发货");
}

#[test]
fn no_placeholders_is_passthrough() {
    let result = render("系统通知：服务已重启", &[]);
    assert_eq!(result, "系统通知：服务已重启");
}

#[test]
fn variables_fall_through_to_annotations() {
    let labels = vars(&[("locatorCode", "L001")]);
    let annotations = vars(&[("lat", "24.4659"), ("lon", "109.7192")]);
    let all = PriorityVariables::new().layer(&labels).layer(&annotations);
    assert_eq!(
        render_with("定位卡 {{locatorCode}} 位于 {{lat}},{{lon}}", &all),
        "定位卡 L001 位于 24.4659,109.7192"
    );
}

#[test]
fn labels_win_on_conflict() {
    let labels = vars(&[("locatorCode", "L001")]);
    let annotations = vars(&[("locatorCode", "L999")]);
    let all = PriorityVariables::new().layer(&labels).layer(&annotations);
    assert_eq!(render_with("{{locatorCode}}", &all), "L001");
}

#[test]
fn builtin_wins_over_labels() {
    let labels = vars(&[("_create_ms", "0")]);
    let builtins: HashMap<&'static str, String> =
        HashMap::from([("_create_ms", "1735689600000".to_string())]);
    let all = PriorityVariables::new().layer(&builtins).layer(&labels);
    assert_eq!(render_with("{{_create_ms}}", &all), "1735689600000");
}

#[test]
fn builtin_accepts_filters() {
    assert_eq!(
        render_with_builtins(
            "创建于 {{_create_ms:time(%Y-%m-%d %H:%M)}}",
            &[],
            &[("_create_ms", "1735689600000")]
        ),
        "创建于 2025-01-01 08:00"
    );
}

#[test]
fn json_escapes_quotes() {
    let labels = vars(&[("deviceName", "1F \"东\" 区烟感")]);
    let template = r#"{"text":{{deviceName:json}}}"#;
    let result = render_with(template, &labels);
    assert_eq!(result, r#"{"text":"1F \"东\" 区烟感"}"#);
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("渲染结果不是合法 JSON");
    assert_eq!(parsed["text"], "1F \"东\" 区烟感");
}

#[test]
fn bare_numeric_placeholder_is_a_number() {
    let annotations = vars(&[("lon", "113.9"), ("lat", "22.5")]);
    let template = r#"{"lon":{{lon}},"lat":{{lat}}}"#;
    let result = render_with(template, &annotations);
    assert_eq!(result, r#"{"lon":113.9,"lat":22.5}"#);
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("渲染结果不是合法 JSON");
    assert_eq!(parsed["lon"], 113.9);
}

#[test]
fn json_missing_variable_is_null() {
    let labels = vars(&[("locatorCode", "L001")]);
    let template = r#"{"code":{{locatorCode:json}},"level":{{floor:json}}}"#;
    let result = render_with(template, &labels);
    assert_eq!(result, r#"{"code":"L001","level":null}"#);
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("渲染结果不是合法 JSON");
    assert!(parsed["level"].is_null());
}

#[test]
fn json_empty_string_stays_empty_string() {
    let labels = vars(&[("floor", "")]);
    assert_eq!(render_with("{{floor:json}}", &labels), r#""""#);
}

#[test]
fn default_null_then_json_is_quoted_null() {
    // 兼容旧语法的边角语义：default 先产出字符串 "null"，json 再给它加引号
    assert_eq!(render("{{x:default(null)|json}}", &[]), r#""null""#);
}

#[test]
fn time_default_format() {
    assert_eq!(
        render("触发时间 {{triggerMs:time}}", &[("triggerMs", "1735689600000")]),
        "触发时间 2025-01-01 08:00:00"
    );
}

#[test]
fn time_custom_format() {
    assert_eq!(
        render(
            "{{triggerMs:time(%Y年%m月%d日 %H:%M:%S)}}",
            &[("triggerMs", "1735689600000")]
        ),
        "2025年01月01日 08:00:00"
    );
}

#[test]
fn time_accepts_seconds() {
    assert_eq!(
        render("{{triggerMs:time}}", &[("triggerMs", "1735689600")]),
        "2025-01-01 08:00:00"
    );
}

#[test]
fn time_then_json() {
    let template = r#"{"time":{{triggerMs:time(%F %T)|json}}}"#;
    let result = render(template, &[("triggerMs", "1735689600000")]);
    assert_eq!(result, r#"{"time":"2025-01-01 08:00:00"}"#);
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("渲染结果不是合法 JSON");
    assert_eq!(parsed["time"], "2025-01-01 08:00:00");
}

#[test]
fn time_on_non_numeric_is_skipped() {
    let template = r#"{"time":{{triggerMs:time|json}}}"#;
    assert_eq!(render(template, &[("triggerMs", "刚刚")]), r#"{"time":"刚刚"}"#);
}

#[test]
fn time_json_missing_variable_is_null() {
    let template = r#"{"time":{{triggerMs:time(%F %T)|json}}}"#;
    assert_eq!(render(template, &[]), r#"{"time":null}"#);
}

#[test]
fn unknown_filter_keeps_placeholder() {
    assert_eq!(
        render("{{triggerMs:times}}", &[("triggerMs", "1735689600000")]),
        "{{triggerMs:times}}"
    );
}

#[test]
fn unclosed_paren_keeps_placeholder() {
    assert_eq!(
        render("{{t:time(%F %T}}", &[("t", "1735689600000")]),
        "{{t:time(%F %T}}"
    );
}

#[test]
fn old_colon_arg_syntax_keeps_placeholder() {
    assert_eq!(
        render("{{t:time:%F}}", &[("t", "1735689600000")]),
        "{{t:time:%F}}"
    );
}

#[test]
fn upper_lower_trim() {
    assert_eq!(render("{{code:upper}}", &[("code", "ab-1")]), "AB-1");
    assert_eq!(render("{{code:lower}}", &[("code", "AB-1")]), "ab-1");
    assert_eq!(render("[{{name:trim}}]", &[("name", "  张三 ")]), "[张三]");
    assert_eq!(render("{{name:upper}}", &[("name", "张三")]), "张三");
}

#[test]
fn filter_chain() {
    assert_eq!(
        render("{{code:trim|upper}}", &[("code", "  alarm-01  ")]),
        "ALARM-01"
    );
    assert_eq!(render("{{code:trim | upper}}", &[("code", " a ")]), "A");
}

#[test]
fn default_filter() {
    assert_eq!(render("{{floor:default(未知)}}", &[]), "未知");
    assert_eq!(render("{{floor:default(未知)}}", &[("floor", "   ")]), "未知");
    assert_eq!(render("{{floor:default(未知)}}", &[("floor", "3F")]), "3F");
}

#[test]
fn arg_may_contain_pipe() {
    assert_eq!(render("{{v:default(a|b)}}", &[]), "a|b");
}

#[test]
fn truncate_by_chars() {
    assert_eq!(
        render("{{desc:truncate(4)}}", &[("desc", "一二三四五六")]),
        "一二三四…"
    );
    assert_eq!(render("{{desc:truncate(10)}}", &[("desc", "一二三")]), "一二三");
}

#[test]
fn fixed_decimals() {
    assert_eq!(
        render("{{lat:fixed(2)}}", &[("lat", "24.465912345")]),
        "24.47"
    );
    assert_eq!(render("{{lat:fixed(0)}}", &[("lat", "24.6")]), "25");
    assert_eq!(render("{{lat:fixed(2)}}", &[("lat", "未知")]), "未知");
}

#[test]
fn mask_keeps_head_and_tail() {
    assert_eq!(
        render("{{phone:mask(3,4)}}", &[("phone", "13800138000")]),
        "138****8000"
    );
    assert_eq!(render("{{name:mask(1,0)}}", &[("name", "张三丰")]), "张**");
    assert_eq!(render("{{phone:mask(3,4)}}", &[("phone", "138")]), "***");
}

#[test]
fn url_encode() {
    assert_eq!(
        render("{{q:url}}", &[("q", "1F 东区&x=1")]),
        "1F%20%E4%B8%9C%E5%8C%BA%26x%3D1"
    );
    assert_eq!(render("{{q:url}}", &[("q", "a-b_c.d~e")]), "a-b_c.d~e");
}

#[test]
fn since_is_deterministic_with_context_now() {
    let now = ctx().now_ms;
    let cases = [
        (now - 1_000, "刚刚"),
        (now - 5 * 60_000, "5 分钟前"),
        (now - 3 * 3_600_000, "3 小时前"),
        (now - 2 * 86_400_000, "2 天前"),
        (now - 45 * 86_400_000, "1 个月前"),
        (now + 60_000, "刚刚"),
    ];
    for (ts, expected) in cases {
        assert_eq!(render("{{t:since}}", &[("t", &ts.to_string())]), expected);
    }
}

#[test]
fn bad_arg_skips_only_that_filter() {
    assert_eq!(render("{{desc:truncate(abc)}}", &[("desc", "一二三")]), "一二三");
    assert_eq!(
        render(r#"{"v":{{v:fixed(x)|json}}}"#, &[("v", "1.5")]),
        r#"{"v":"1.5"}"#
    );
}

#[test]
fn unexpected_arg_is_skipped() {
    assert_eq!(render("{{code:upper(1)}}", &[("code", "ab")]), "ab");
}

#[test]
fn empty_filter_spec_is_identity() {
    assert_eq!(render("{{code:}}", &[("code", "ab")]), "ab");
}

#[test]
fn double_braces_are_single_pass_and_not_rescanned() {
    // 变量值里带模板语法时，不能二次渲染
    let labels = vars(&[("v", "{{name}}"), ("name", "张三")]);
    let all = PriorityVariables::new().layer(&labels);
    assert_eq!(render_with("{{v}}", &all), "{{name}}");
}

#[test]
fn render_does_not_touch_legacy_single_braces() {
    // 旧语法已废弃：即使变量存在，也不应该被替换
    let labels = vars(&[("name", "张三")]);
    assert_eq!(render_with("你好 {name}", &labels), "你好 {name}");
}

#[test]
fn mixed_legacy_and_new_syntax() {
    let labels = vars(&[("name", "张三")]);
    assert_eq!(
        render_with("{{name}} / {name}", &labels),
        "张三 / {name}"
    );
}

#[test]
fn json_object_braces_are_untouched() {
    let labels = vars(&[("name", "张三")]);
    assert_eq!(
        render_with(r#"{"k":{{name:json}},"n":1}"#, &labels),
        r#"{"k":"张三","n":1}"#
    );
}

#[test]
fn render_reports_missing_variable_diagnostic() {
    let empty: HashMap<String, String> = HashMap::new();
    let outcome = render_template("{{missing}}", &empty, &ctx());
    assert_eq!(outcome.text, "{{missing}}");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.diagnostics[0].severity, Severity::Warning);
    assert_eq!(
        outcome.diagnostics[0].code,
        DiagnosticCode::MissingVariable {
            key: "missing".to_string()
        }
    );
}

#[test]
fn render_reports_filter_failed_diagnostic() {
    let outcome = render_template(
        "{{t:time}}",
        &vars(&[("t", "不是时间")]),
        &ctx(),
    );
    // 失败被跳过，原值继续往后走，最终仍然有值
    assert_eq!(outcome.text, "不是时间");
    assert_eq!(outcome.diagnostics.len(), 1);
    assert!(matches!(
        outcome.diagnostics[0].code,
        DiagnosticCode::FilterFailed { .. }
    ));
}

#[test]
fn filter_registry_contains_all_eleven_filters() {
    let names = filter_names();
    assert_eq!(names.len(), 11);
    for name in [
        "json", "time", "since", "upper", "lower", "trim", "default", "truncate", "fixed",
        "mask", "url",
    ] {
        assert!(is_known_filter(name), "{name} 未注册");
    }
}

#[test]
fn validate_accepts_new_syntax() {
    let check = validate_template(
        r#"{"code":{{locatorCode:json}},"time":{{triggerMs:time(%F %T)|json}}}"#,
    );
    assert!(!check.has_errors(), "{:?}", check.diagnostics);
    assert_eq!(check.placeholders.len(), 2);
    assert_eq!(check.placeholders[0].key, "locatorCode");
    assert_eq!(check.placeholders[1].filters.len(), 2);
}

#[test]
fn validate_flags_unknown_filter() {
    let check = validate_template("{{x:uppper}}");
    assert!(check.has_errors());
    assert!(matches!(
        check.diagnostics[0].code,
        DiagnosticCode::UnknownFilter { .. }
    ));
}

#[test]
fn validate_flags_invalid_filter_syntax() {
    let check = validate_template("{{t:time:%F}}");
    assert!(check.has_errors());
    assert!(matches!(
        check.diagnostics[0].code,
        DiagnosticCode::InvalidFilterSyntax { .. }
    ));
}

#[test]
fn validate_flags_unclosed_placeholder() {
    let check = validate_template("你好 {{name");
    assert!(check.has_errors());
    assert_eq!(
        check.diagnostics[0].code,
        DiagnosticCode::UnclosedPlaceholder
    );
}

#[test]
fn validate_flags_legacy_syntax() {
    let check = validate_template("你好 {name}");
    assert!(check.has_errors());
    assert_eq!(check.diagnostics[0].code, DiagnosticCode::LegacySyntax);
}

#[test]
fn validate_ignores_json_braces() {
    let check = validate_template(r#"{"a":1,"b":{"c":2}}"#);
    assert!(!check.has_errors(), "{:?}", check.diagnostics);
}

#[test]
fn validate_flags_legacy_syntax_with_filter() {
    let check = validate_template(r#"{"t":{triggerMs:time}}"#);
    assert!(check.has_errors());
    assert_eq!(check.diagnostics[0].code, DiagnosticCode::LegacySyntax);
}

#[test]
fn validate_accepts_plain_text_braces() {
    let check = validate_template("普通文本里的花括号 {不 是 占位符}");
    assert!(!check.has_errors(), "{:?}", check.diagnostics);
}
