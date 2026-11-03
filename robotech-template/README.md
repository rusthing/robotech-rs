# robotech-template

跨服务共用的模板渲染内核：`{{变量}}` / `${变量}` 占位符、过滤器链、结构化诊断、可参数化的变量来源。

本 crate 只提供机制，**不读配置、不写日志、不访问数据库**。时区与「当前时间」
由调用方放进 `FilterContext`，变量优先级由调用方用 `PriorityVariables` 逐层组装，
诊断以 `RenderOutcome` / `TemplateCheck` 返回，由调用方决定告警还是拒绝保存。

## 语法

```text
{{key}}                         原样代入
{{key:过滤器}}                   一个过滤器
{{key:过滤器(参数)}}              带参数的过滤器
{{key:过滤器(参数)|过滤器}}       过滤器链，从左到右执行
```

`${key}` / `${key:过滤器}` / `${key:过滤器(参数)}` / `${key:过滤器(参数)|过滤器}`
是与上面四种写法**完全等价**的别名：同一套过滤器、同一套缺值策略、同样的单遍
扫描。`$` 只是定界符的一部分，**不做任何环境变量或默认值解释**；两种写法可以在
同一模板里混用，按出现顺序逐个替换，互不干扰。

**旧语法 `{key}` 已废弃，不再支持。** 单花括号占位符不会被替换，会原样留在
渲染结果里；`validate_template` 会把它报成 `DiagnosticCode::LegacySyntax`。

模板里的普通花括号（例如 JSON 对象的 `{"k":1}`）不受影响：只有 `{{` 开头、`}}`
结尾（或 `${` 开头、`}` 结尾）且中间是合法变量名/过滤器链的片段才会被识别为
占位符。

## 内置过滤器

| 过滤器 | 参数 | 作用 |
|---|---|---|
| `json` | — | 转成 JSON 字符串字面量（自带引号并转义）；缺值输出 `null` |
| `time` | strftime 格式，缺省 `%Y-%m-%d %H:%M:%S` | 时间戳 → 可读时间 |
| `since` | — | 时间戳 → 相对时间（`3 分钟前`） |
| `upper` / `lower` | — | 大小写转换 |
| `trim` | — | 去首尾空白 |
| `default` | 兜底文本 | 变量缺失或值为空白时替换成兜底文本 |
| `truncate` | 字符数 | 按字符截断，超长补 `…` |
| `fixed` | 小数位 | 数字按固定小数位输出，非数字原样 |
| `mask` | `首,尾` 位数 | 脱敏：保留首尾若干字符，中间换成 `*` |
| `url` | — | percent-encode（空格编成 `%20`） |

## 缺值与失败语义

- **变量缺失**：链里没有 `default` / `json` 产出值时，保留原占位符并产生
  `MissingVariable` 诊断；`json` 会把缺值变成 `null`。
- **过滤器名写错或语法不对**：整条占位符保留原文，产生 `UnknownFilter` /
  `InvalidFilterSyntax` 诊断。
- **单个过滤器执行失败**（如 `time` 遇到非数字）：跳过它，把当前值交给后面的
  过滤器，产生 `FilterFailed` 诊断。

## 用法

```rust
use chrono::FixedOffset;
use robotech_template::{FilterContext, PriorityVariables, render_template};

let builtins = std::collections::HashMap::from([("_create_ms", "1735689600000".to_string())]);
let labels = std::collections::HashMap::from([("locatorCode", "L001".to_string())]);
let annotations = std::collections::HashMap::new();

let vars = PriorityVariables::new()
    .layer(&builtins)
    .layer(&labels)
    .layer(&annotations);
let ctx = FilterContext::new(
    FixedOffset::east_opt(8 * 3600).unwrap(),
    1_735_689_700_000,
);

let outcome = render_template("定位卡 {{locatorCode}} 于 {{_create_ms:time}} 更新", &vars, &ctx);
assert_eq!(outcome.text, "定位卡 L001 于 2025-01-01 08:00:00 更新");
```

保存模板时可调用 `validate_template`：未闭合的 `{{` / `${`、残留旧语法、过滤器语法错误、
未知过滤器都会以 Error 级诊断返回；`{{key}}` 与 `${key}` 都会被识别为合法占位符。
