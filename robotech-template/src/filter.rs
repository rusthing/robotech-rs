//! 过滤器注册表与 11 个内置过滤器
//!
//! 过滤器只做纯计算：不读配置、不打日志、不访问外部资源。执行失败以 `Err`
//! 返回，由渲染引擎转成结构化诊断并跳过该过滤器。

use chrono::{FixedOffset, TimeZone, Utc};
use std::collections::HashMap;
use std::sync::LazyLock;

/// `time` 不带格式串时使用的默认格式
const DEFAULT_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// 秒级与毫秒级时间戳的分界
///
/// `1e11` 毫秒是 1973 年，`1e11` 秒是公元 5138 年——两个量级不会撞车，
/// 小于这个数的一律当秒。发布方把秒级时间戳塞进 `labels` 是常见手滑。
const SECONDS_TS_UPPER_BOUND: u64 = 100_000_000_000;

/// 过滤器执行上下文
///
/// 时区与「当前时间」都由调用方传入：crate 不读配置，也不能在测试里依赖
/// 真实时钟，否则 `since` 的结果不可断言。
#[derive(Debug, Clone, Copy)]
pub struct FilterContext {
    /// 模板时区偏移（`time` 用）
    pub time_offset: FixedOffset,
    /// 渲染时刻（毫秒），`since` 用
    pub now_ms: i64,
}

impl FilterContext {
    /// 构造上下文
    pub const fn new(time_offset: FixedOffset, now_ms: i64) -> Self {
        Self {
            time_offset,
            now_ms,
        }
    }
}

/// 过滤器函数
///
/// * `value` - 当前值。`None` 表示变量缺失，或前序过滤器判定为「无值」
/// * `arg` - `名字(参数)` 里的参数，无参数时为 `None`
/// * `ctx` - 执行上下文
///
/// 返回：
/// * `Ok(Some(v))` - 正常处理，`v` 交给下一个过滤器
/// * `Ok(None)` - 仍然无值，引擎最终按「变量缺失」处理
/// * `Err(原因)` - 这个过滤器做不了。引擎会记录诊断并跳过它，保留当前值
pub type FilterFn = fn(Option<&str>, Option<&str>, &FilterContext) -> Result<Option<String>, String>;

/// 过滤器注册表：模板里 `{{key:名字}}` 的「名字」在这里查
static FILTERS: LazyLock<HashMap<&'static str, FilterFn>> = LazyLock::new(|| {
    HashMap::from([
        ("json", filter_json as FilterFn),
        ("time", filter_time),
        ("since", filter_since),
        ("upper", filter_upper),
        ("lower", filter_lower),
        ("trim", filter_trim),
        ("default", filter_default),
        ("truncate", filter_truncate),
        ("fixed", filter_fixed),
        ("mask", filter_mask),
        ("url", filter_url),
    ])
});

/// 查询一个过滤器
pub(crate) fn lookup(name: &str) -> Option<FilterFn> {
    FILTERS.get(name).copied()
}

/// 过滤器名是否已注册
pub fn is_known_filter(name: &str) -> bool {
    FILTERS.contains_key(name)
}

/// 全部内置过滤器名（按字典序）
pub fn filter_names() -> Vec<&'static str> {
    let mut names: Vec<_> = FILTERS.keys().copied().collect();
    names.sort_unstable();
    names
}

/// 断言过滤器不带参数
fn no_arg(arg: Option<&str>, name: &str) -> Result<(), String> {
    match arg {
        None => Ok(()),
        Some(arg) if arg.trim().is_empty() => Ok(()),
        Some(arg) => Err(format!("{name} 不接受参数（给了 {arg}）")),
    }
}

/// 解析参数里的非负整数
fn parse_arg_usize(arg: Option<&str>, hint: &str) -> Result<usize, String> {
    let arg = arg
        .filter(|a| !a.trim().is_empty())
        .ok_or_else(|| hint.to_owned())?;
    arg.trim()
        .parse::<usize>()
        .map_err(|_| format!("参数 {arg} 不是非负整数"))
}

/// # 解析时间戳字符串为毫秒
///
/// 秒级 / 毫秒级自动识别（见 [`SECONDS_TS_UPPER_BOUND`]），并挡住超出
/// `i64` 表示范围的值——`as i64` 静默回绕会变成 1970 年前。
fn parse_ts_ms(value: &str) -> Result<i64, String> {
    let raw = value
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("{value} 不是时间戳"))?;
    let ms = if raw < SECONDS_TS_UPPER_BOUND {
        raw.saturating_mul(1000)
    } else {
        raw
    };
    if ms > i64::MAX as u64 {
        return Err(format!("{value} 超出可表示范围"));
    }
    Ok(ms as i64)
}

/// `json`：把值转成 JSON 字符串字面量
///
/// 缺失时输出 `null` 而不是保持缺失——`ext` 模板是 JSON，结构不能塌。
fn filter_json(
    value: Option<&str>,
    arg: Option<&str>,
    _ctx: &FilterContext,
) -> Result<Option<String>, String> {
    no_arg(arg, "json")?;
    Ok(Some(match value {
        // 值本身带引号、反斜杠、换行时，不转义就会把外层的 JSON 撑破
        Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "null".to_owned()),
        None => "null".to_owned(),
    }))
}

/// `time`：时间戳 → 可读时间
///
/// 秒级 / 毫秒级自动识别，按 `ctx.time_offset` 换算时区。
fn filter_time(
    value: Option<&str>,
    arg: Option<&str>,
    ctx: &FilterContext,
) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let fmt = arg.filter(|f| !f.is_empty()).unwrap_or(DEFAULT_TIME_FORMAT);
    let ms = parse_ts_ms(value)?;

    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|dt| Some(dt.with_timezone(&ctx.time_offset).format(fmt).to_string()))
        .ok_or_else(|| format!("{value} 超出可表示范围"))
}

/// # 把毫秒差描述成相对时间
///
/// 告警文案里 `3 分钟前` 比时间戳直观。未来时间（时钟偏差）一律按「刚刚」，
/// 免得出现「-1 分钟前」这种让人以为系统坏了的输出。
fn humanize_gap(diff_ms: i64) -> String {
    const MINUTE: i64 = 60_000;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;

    if diff_ms < MINUTE {
        return "刚刚".to_owned();
    }
    if diff_ms < HOUR {
        return format!("{} 分钟前", diff_ms / MINUTE);
    }
    if diff_ms < DAY {
        return format!("{} 小时前", diff_ms / HOUR);
    }
    if diff_ms < 30 * DAY {
        return format!("{} 天前", diff_ms / DAY);
    }
    if diff_ms < 365 * DAY {
        return format!("{} 个月前", diff_ms / (30 * DAY));
    }
    format!("{} 年前", diff_ms / (365 * DAY))
}

/// `since`：时间戳 → 相对时间
///
/// 相对时间是**渲染时刻**算出来的，同一条消息晚一点重放会变成另一个字符串。
/// 需要可追溯的快照就别用它：`content` 里用 `time`，`ext` 里把原始时间戳
/// 裸代入（`{{_create_ms}}` / `{{_business_trigger_ms}}`）留给消费方自己算差。
fn filter_since(
    value: Option<&str>,
    arg: Option<&str>,
    ctx: &FilterContext,
) -> Result<Option<String>, String> {
    no_arg(arg, "since")?;
    let Some(value) = value else {
        return Ok(None);
    };
    let ms = parse_ts_ms(value)?;
    Ok(Some(humanize_gap(ctx.now_ms.saturating_sub(ms))))
}

/// `upper`：转大写
fn filter_upper(
    value: Option<&str>,
    arg: Option<&str>,
    _ctx: &FilterContext,
) -> Result<Option<String>, String> {
    no_arg(arg, "upper")?;
    Ok(value.map(str::to_uppercase))
}

/// `lower`：转小写
fn filter_lower(
    value: Option<&str>,
    arg: Option<&str>,
    _ctx: &FilterContext,
) -> Result<Option<String>, String> {
    no_arg(arg, "lower")?;
    Ok(value.map(str::to_lowercase))
}

/// `trim`：去首尾空白
///
/// 发布方拼字符串时经常带出多余空格，短信/标题里很显眼。
fn filter_trim(
    value: Option<&str>,
    arg: Option<&str>,
    _ctx: &FilterContext,
) -> Result<Option<String>, String> {
    no_arg(arg, "trim")?;
    Ok(value.map(|v| v.trim().to_owned()))
}

/// `default`：变量缺失或值为空白时换成兜底文本
///
/// 空白也算缺失：`labels` 里塞了空串是常见情况，直接代入会渲染出一个洞。
fn filter_default(
    value: Option<&str>,
    arg: Option<&str>,
    _ctx: &FilterContext,
) -> Result<Option<String>, String> {
    let fallback =
        arg.ok_or_else(|| "default 需要兜底文本，如 {{key:default(未知)}}".to_owned())?;
    Ok(match value {
        Some(v) if !v.trim().is_empty() => Some(v.to_owned()),
        _ => Some(fallback.to_owned()),
    })
}

/// `truncate`：按字符截断，超长时补 `…`
///
/// 按字符而不是字节：中文按字节切会切出半个字，直接 panic 或乱码。
fn filter_truncate(
    value: Option<&str>,
    arg: Option<&str>,
    _ctx: &FilterContext,
) -> Result<Option<String>, String> {
    let limit = parse_arg_usize(arg, "truncate 需要字符数，如 {{key:truncate(10)}}")?;
    Ok(value.map(|v| {
        if v.chars().count() <= limit {
            v.to_owned()
        } else {
            v.chars().take(limit).chain(std::iter::once('…')).collect()
        }
    }))
}

/// `fixed`：数字按固定小数位输出
///
/// 坐标这类值发布方可能给 `24.465912345`，投到短信里没必要这么长。
/// 非数字原样返回，但不打日志——诊断交给引擎。
fn filter_fixed(
    value: Option<&str>,
    arg: Option<&str>,
    _ctx: &FilterContext,
) -> Result<Option<String>, String> {
    let digits = parse_arg_usize(arg, "fixed 需要小数位，如 {{key:fixed(2)}}")?;
    Ok(value.map(|v| match v.trim().parse::<f64>() {
        Ok(num) => format!("{num:.digits$}"),
        Err(_) => v.to_owned(),
    }))
}

/// `mask`：脱敏，保留首尾若干字符，中间换成 `*`
///
/// 参数形如 `3,4`（保留前 3 位、后 4 位），手机号 `13800138000` 渲染成
/// `138****8000`。字符数不超过首尾之和时整串打码。
fn filter_mask(
    value: Option<&str>,
    arg: Option<&str>,
    _ctx: &FilterContext,
) -> Result<Option<String>, String> {
    let arg = arg
        .filter(|a| !a.trim().is_empty())
        .ok_or_else(|| "mask 需要保留位数，如 {{key:mask(3,4)}}".to_owned())?;
    let (head, tail) = arg
        .split_once(',')
        .ok_or_else(|| format!("mask 的参数 {arg} 应形如 3,4"))?;
    let head = parse_arg_usize(Some(head), "mask 缺少前段保留位数")?;
    let tail = parse_arg_usize(Some(tail), "mask 缺少后段保留位数")?;

    Ok(value.map(|v| {
        let chars: Vec<char> = v.chars().collect();
        if chars.len() <= head + tail {
            return "*".repeat(chars.len());
        }
        let mut masked: String = chars[..head].iter().collect();
        masked.push_str(&"*".repeat(chars.len() - head - tail));
        masked.push_str(&chars[chars.len() - tail..].iter().collect::<String>());
        masked
    }))
}

/// # 按 RFC 3986 的 unreserved 规则做 percent-encode
///
/// 空格编成 `%20` 而不是 `+`：`+` 只在 `application/x-www-form-urlencoded`
/// 里有「空格」的含义，拼查询串之外的场景会变成字面量加号。
fn percent_encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    let mut encoded = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0F) as usize] as char);
            }
        }
    }
    encoded
}

/// `url`：percent-encode，用于把变量拼进链接的查询参数
fn filter_url(
    value: Option<&str>,
    arg: Option<&str>,
    _ctx: &FilterContext,
) -> Result<Option<String>, String> {
    no_arg(arg, "url")?;
    Ok(value.map(percent_encode))
}
