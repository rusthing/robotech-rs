//! # 字段格式校验适配
//!
//! `#[crud_dto]` 把字段注释里的 `@check is_xxx` 翻译成
//! `#[validate(custom(function = "robotech::api::validators::xxx"))]`，
//! 指向本模块的**同名去掉 `is_` 前缀**的函数。
//!
//! 这一层适配存在的理由有两条：
//!
//! 1. [`wheel_rs::regex_utils`] 的 `is_*` 返回 `bool`，而 validator 的 `custom`
//!    要求 `Result<(), ValidationError>`——总不能在业务 crate 里为每个字段手写包装
//! 2. 同一个 `@check` 可能挂在 `String` 上，也可能挂在 `Option<String>` 上：
//!    validator 的 `custom` 只认 `fn(&str)`，可空字段的 `None` 由 validator
//!    在进函数**之前**跳过，所以函数本身只需处理 `&str`；[`CheckTarget`] 是给
//!    业务侧自己判断「有没有值」用的，两种形态共用 `check_str`
//!
//! ## 与 `@len` / `@range` 的分工
//!
//! * `@len` / `@range` 是**结构性**约束，validator 有原生规则，宏直接生成
//! * `@check` 是**语义**约束（带校验位、要解析、要多步判断），只能调函数
//!
//! 因此 `@check` 比正则严：`is_id_card_cn` 会算 ISO 7064 校验位并检查出生日期，
//! 而正则只能对上 18 位的形状。
//!
//! ## 提示语：格式 + 实际收到的值
//!
//! 每条提示语是 `{字段中文名}格式不正确：应为 {格式}，实际收到 {值}`，例如
//!
//! ```text
//! 手机号格式不正确：应为 ^1[3-9]\d{9}$，实际收到 "1281234567"
//! ```
//!
//! 「格式」尽量直接给 `wheel_rs::regex_utils::pattern` 里的正则——那本来就是格式的
//! 唯一定义，比「11 位数字、1 开头」这类转述准确；正则表达不了的部分（校验位、
//! 要解析的）写一句短的。
//!
//! **密钥、令牌、盐这类值不回显**：DTO 注释里标了 `@secret` 的字段，
//! `#[crud_dto]` 会改用 [`hidden`] 下的同名函数——提示语只留格式，不带值。
//! 判定放在注释里而不是这里，是因为同一个规则（比如 `is_code`）既可能校验
//! 公开编码，也可能校验令牌，单看函数名分不出来。
//!
//! ## 示例
//!
//! ```
//! use robotech::api::validators::{email, hidden, id_card_cn};
//!
//! assert!(email("zhang.san@example.com").is_ok());
//! assert!(email("not-an-email").is_err());
//! // 校验位错了要拦住：正则只认 18 位的形状
//! assert!(id_card_cn("11010519491231002X").is_ok());
//! assert!(id_card_cn("110105194912310021").is_err());
//! // `@secret` 字段走 hidden：提示语里不带值
//! assert!(hidden::email("bad").is_err());
//! ```
//!
//! 可空字段不必在这里演示：validator 对 `Option` 字段生成的是
//! `if let Some(ref v) = self.field { ... }`，`None` 直接跳过（是否必填由
//! `required` 规则负责），`Some` 解出的 `&String` 靠 deref coercion 落进 `&str` 形参。

use std::borrow::Cow;

use validator::ValidationError;
use wheel_rs::regex_utils::pattern;

/// # 校验目标
///
/// 保留这个 trait 是为了让业务侧能复用同一套函数做「有没有值」的判断，
/// 但 validator 的 `custom` 只认 `fn(&str)`：可空字段的 `None` 由 validator 自己跳过，
/// 不会走到函数里。
pub trait CheckTarget {
    /// 取出待校验的字符串；`None` 表示这个字段没有值
    fn check_str(&self) -> Option<&str>;
}

impl CheckTarget for String {
    fn check_str(&self) -> Option<&str> {
        Some(self.as_str())
    }
}

impl CheckTarget for Option<String> {
    fn check_str(&self) -> Option<&str> {
        self.as_deref()
    }
}

/// 提示语里回显原值时的字符上限
///
/// 值长度由调用方决定，原样回显等于让别人决定日志行和响应体有多长。
const ECHO_MAX_CHARS: usize = 32;

/// # 把实际收到的值渲染成可安全写进提示语的一小段文本
///
/// * 控制字符转义（`escape_debug`）：提示语会进日志，不能让人用换行伪造日志行
/// * 超长截断到 [`ECHO_MAX_CHARS`] 个字符，并注明总字符数
///
/// 请求头那边也用这一份（`ctrl_utils` 先做 lossy 转换再传进来），免得两处各写一套
/// 截断规则。
pub(crate) fn echo_value(raw: &str) -> String {
    let mut shown = String::new();
    let mut truncated = false;
    for (idx, ch) in raw.chars().enumerate() {
        if idx == ECHO_MAX_CHARS {
            truncated = true;
            break;
        }
        shown.push(ch);
    }
    let escaped = shown.escape_debug();
    if truncated {
        format!("\"{escaped}…\"（共 {} 字符）", raw.chars().count())
    } else {
        format!("\"{escaped}\"")
    }
}

/// 是否在提示语里回显原值
#[derive(Clone, Copy)]
enum Echo {
    /// 回显（业务字段）
    Show,
    /// 不回显（`@secret`：密钥、令牌、盐）
    Hide,
}

/// 组装一条「格式不正确」的校验错误
fn format_error(
    code: &'static str,
    label: &str,
    expected: &str,
    echo: Echo,
    value: &str,
) -> ValidationError {
    let message = match echo {
        Echo::Show => format!(
            "{label}格式不正确：应为 {expected}，实际收到 {}",
            echo_value(value)
        ),
        Echo::Hide => format!("{label}格式不正确：应为 {expected}"),
    };
    let mut error = ValidationError::new(code);
    error.message = Some(Cow::Owned(message));
    error
}

/// 生成一个「回显原值」的校验函数
///
/// 名字与 `wheel_rs::regex_utils::is_*` 一一对应（去掉 `is_` 前缀），
/// 这样 `@check is_email` 能直接映射到 `validators::email`。
///
/// 签名固定为 `fn(&str) -> Result<(), ValidationError>`：这是 validator 的
/// `custom` 能接的形状，`String` 与 `Option<String>` 都靠 deref coercion 落进来。
macro_rules! check_fn {
    ($(#[$meta:meta])* $name:ident => $pred:path, $label:literal, $expected:expr) => {
        $(#[$meta])*
        pub fn $name(value: &str) -> Result<(), ValidationError> {
            if $pred(value) {
                return Ok(());
            }
            Err(format_error(
                concat!("invalid_", stringify!($name)),
                $label,
                $expected,
                Echo::Show,
                value,
            ))
        }
    };
}

/// 生成一个「不回显原值」的校验函数，签名与 [`check_fn`] 完全一致
macro_rules! check_fn_hidden {
    ($(#[$meta:meta])* $name:ident => $pred:path, $label:literal, $expected:expr) => {
        $(#[$meta])*
        pub fn $name(value: &str) -> Result<(), ValidationError> {
            if $pred(value) {
                return Ok(());
            }
            Err(format_error(
                concat!("invalid_", stringify!($name)),
                $label,
                $expected,
                Echo::Hide,
                value,
            ))
        }
    };
}

/// 校验规则的唯一清单
///
/// 只在这里写一遍：`validators::xxx` 与 [`hidden`]`::xxx` 各展开一次，
/// 两套除「回不回显」之外完全一致。新增规则加在这里，不要只加一边——
/// `@secret` 字段一旦指向 `hidden` 里不存在的函数，编译会直接报错。
macro_rules! each_check {
    ($define:ident) => {
        $define!(
            /// 邮箱地址（RFC 5322 简化版，总长 ≤254、本地部分 ≤64）
            email => wheel_rs::regex_utils::is_email, "邮箱", pattern::EMAIL
        );
        $define!(
            /// 中国大陆手机号（`1[3-9]` + 9 位数字）
            mobile_cn => wheel_rs::regex_utils::is_mobile_cn, "手机号", pattern::MOBILE_CN
        );
        $define!(
            /// 中国大陆固定电话（区号 `0` + 2~3 位，号码 7~8 位，可选分机）
            landline_cn => wheel_rs::regex_utils::is_landline_cn, "固定电话", pattern::LANDLINE_CN
        );
        $define!(
            /// 18 位身份证号（含出生日期合法性与 ISO 7064 校验位）
            id_card_cn => wheel_rs::regex_utils::is_id_card_cn, "身份证号", "18 位身份证号（含校验位）"
        );
        $define!(
            /// 统一社会信用代码（含 GB 32100 模 31 校验位）
            uscc => wheel_rs::regex_utils::is_uscc, "统一社会信用代码", "18 位统一社会信用代码（含校验位）"
        );
        $define!(
            /// 银行卡号（16~19 位 + Luhn 校验）
            bank_card => wheel_rs::regex_utils::is_bank_card, "银行卡号", "16~19 位数字（含 Luhn 校验）"
        );
        $define!(
            /// 车牌号码（省份简称 + 发牌机关字母 + 序号，7/8 位，只认大写）
            plate_cn => wheel_rs::regex_utils::is_plate_cn, "车牌号", pattern::PLATE_CN
        );
        $define!(
            /// `http://` / `https://` 开头的绝对 URL
            url_http => wheel_rs::regex_utils::is_url_http, "URL", pattern::URL_HTTP
        );
        $define!(
            /// 树编码（每层固定 3 位数字，1~10 层）
            tree_code => wheel_rs::regex_utils::is_tree_code, "树编码", pattern::TREE_CODE
        );
        $define!(
            /// UUID（8-4-4-4-12，忽略大小写）
            uuid => wheel_rs::regex_utils::is_uuid, "UUID", pattern::UUID
        );
        $define!(
            /// 登录名（字母开头，字母/数字/下划线，4~32 位）
            account_name => wheel_rs::regex_utils::is_account_name, "登录名", pattern::ACCOUNT_NAME
        );
        $define!(
            /// 通用编码（字母/数字/`_`/`.`/`-`，1~50 位）
            code => wheel_rs::regex_utils::is_code, "编码", pattern::CODE
        );
        $define!(
            /// 中文姓名（汉字与间隔号 `·`，2~30 字）
            chinese_name => wheel_rs::regex_utils::is_chinese_name, "姓名", pattern::CHINESE_NAME
        );
        $define!(
            /// 口令（8~64 位可打印 ASCII，同时含字母与数字）
            password => wheel_rs::regex_utils::is_password, "口令", "8~64 位可打印字符，且同时含字母与数字"
        );
        $define!(
            /// IPv4 或 IPv6 地址（用 `std::net` 解析，比正则严）
            ip => wheel_rs::regex_utils::is_ip, "IP 地址", "IPv4 或 IPv6 地址"
        );
        $define!(
            /// IPv4 地址
            ipv4 => wheel_rs::regex_utils::is_ipv4, "IPv4 地址", pattern::IPV4
        );
        $define!(
            /// IPv6 地址
            ipv6 => wheel_rs::regex_utils::is_ipv6, "IPv6 地址", "IPv6 地址"
        );
        $define!(
            /// 区域轮廓 WKT（结构级：环闭合、每环 ≥4 点；数值范围/自相交不在文本校验内）
            wkt_polygon => wheel_rs::regex_utils::is_wkt_polygon, "区域轮廓", "OGC WKT POLYGON/MULTIPOLYGON"
        );
    };
}

each_check!(check_fn);

/// `@secret` 字段用的校验函数：与根上的同名函数一一对应，只是提示语里不带原值
///
/// 走这里的是密钥、令牌、盐——这些值一旦进了提示语，就等于同时进了响应体和日志，
/// 而日志的留存与查看权限通常比接口宽得多。`#[crud_dto]` 见到 `@secret` 才会把
/// `custom` 的路径指到这里；没标的字段仍用根上的版本（带值回显）。
pub mod hidden {
    use super::*;

    each_check!(check_fn_hidden);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_rejects() {
        assert!(email("zhang.san@example.com").is_ok());
        assert!(email("not-an-email").is_err());
    }

    #[test]
    fn plain_string_works_too() {
        assert!(mobile_cn("13812345678").is_ok());
        assert!(mobile_cn("12812345678").is_err());
    }

    #[test]
    fn message_says_format_and_value() {
        let err = email("bad").unwrap_err();
        assert_eq!(
            err.message.as_deref(),
            Some(format!("邮箱格式不正确：应为 {}，实际收到 \"bad\"", pattern::EMAIL).as_str())
        );
        // 值原样回显，不再裹一层解释
        let err = mobile_cn("1281234567").unwrap_err();
        assert!(
            err.message
                .as_deref()
                .unwrap()
                .ends_with("实际收到 \"1281234567\""),
            "{:?}",
            err.message
        );
    }

    #[test]
    fn message_truncates_long_value() {
        let value = "x".repeat(200);
        let err = code(&value).unwrap_err();
        let message = err.message.unwrap();
        assert!(message.contains("…"), "{message}");
        assert!(message.contains("共 200 字符"), "{message}");
        assert!(
            !message.contains(&"x".repeat(ECHO_MAX_CHARS + 1)),
            "{message}"
        );
    }

    #[test]
    fn hidden_variants_keep_format_but_drop_value() {
        // 空格不在 `pattern::CODE` 的字符集里，正好让 is_code 判失败
        let token = "tok en-should-not-appear";
        let shown = code(token).unwrap_err();
        assert!(shown.message.as_deref().unwrap().contains(token));

        let hidden_error = hidden::code(token).unwrap_err();
        let message = hidden_error.message.unwrap();
        assert_eq!(message, format!("编码格式不正确：应为 {}", pattern::CODE));
        assert!(!message.contains(token), "{message}");
        // 规则名与回显版一致：日志侧靠 code 分不出两者，也不需要分
        assert_eq!(hidden_error.code, "invalid_code");
    }

    #[test]
    fn checks_beyond_regex() {
        // 校验位错了要能拦住：正则只认 18 位形状
        assert!(id_card_cn("11010519491231002X").is_ok());
        assert!(id_card_cn("110105194912310021").is_err());
    }

    #[test]
    fn plate_rejects_lowercase() {
        assert!(plate_cn("京AD12345").is_ok());
        // 大小写混入会让库里的唯一约束失效，因此只认大写
        assert!(plate_cn("京a12345").is_err());
    }

    #[test]
    fn check_target_reads_both_shapes() {
        assert_eq!(String::from("x").check_str(), Some("x"));
        assert_eq!(Some(String::from("y")).check_str(), Some("y"));
        assert_eq!(None::<String>.check_str(), None);
    }
}
