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
//! ## 示例
//!
//! ```
//! use robotech::api::validators::{email, id_card_cn};
//!
//! assert!(email("zhang.san@example.com").is_ok());
//! assert!(email("not-an-email").is_err());
//! // 校验位错了要拦住：正则只认 18 位的形状
//! assert!(id_card_cn("11010519491231002X").is_ok());
//! assert!(id_card_cn("110105194912310021").is_err());
//! ```
//!
//! 可空字段不必在这里演示：validator 对 `Option` 字段生成的是
//! `if let Some(ref v) = self.field { ... }`，`None` 直接跳过（是否必填由
//! `required` 规则负责），`Some` 解出的 `&String` 靠 deref coercion 落进 `&str` 形参。

use std::borrow::Cow;

use validator::ValidationError;

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

/// 生成一个 `custom` 校验函数
///
/// 名字与 `wheel_rs::regex_utils::is_*` 一一对应（去掉 `is_` 前缀），
/// 这样 `@check is_email` 能直接映射到 `validators::email`。
///
/// 签名固定为 `fn(&str) -> Result<(), ValidationError>`：这是 validator 的
/// `custom` 能接的形状，`String` 与 `Option<String>` 都靠 deref coercion 落进来。
macro_rules! check_fn {
    ($(#[$meta:meta])* $name:ident => $pred:path, $label:literal) => {
        $(#[$meta])*
        pub fn $name(value: &str) -> Result<(), ValidationError> {
            if $pred(value) {
                return Ok(());
            }
            let mut error = ValidationError::new(concat!("invalid_", stringify!($name)));
            error.message = Some(Cow::from(concat!($label, "格式不正确")));
            Err(error)
        }
    };
}

check_fn!(
    /// 邮箱地址（RFC 5322 简化版，总长 ≤254、本地部分 ≤64）
    email => wheel_rs::regex_utils::is_email, "邮箱"
);
check_fn!(
    /// 中国大陆手机号（`1[3-9]` + 9 位数字）
    mobile_cn => wheel_rs::regex_utils::is_mobile_cn, "手机号"
);
check_fn!(
    /// 中国大陆固定电话（区号 `0` + 2~3 位，号码 7~8 位，可选分机）
    landline_cn => wheel_rs::regex_utils::is_landline_cn, "固定电话"
);
check_fn!(
    /// 18 位身份证号（含出生日期合法性与 ISO 7064 校验位）
    id_card_cn => wheel_rs::regex_utils::is_id_card_cn, "身份证号"
);
check_fn!(
    /// 统一社会信用代码（含 GB 32100 模 31 校验位）
    uscc => wheel_rs::regex_utils::is_uscc, "统一社会信用代码"
);
check_fn!(
    /// 银行卡号（16~19 位 + Luhn 校验）
    bank_card => wheel_rs::regex_utils::is_bank_card, "银行卡号"
);
check_fn!(
    /// 车牌号码（省份简称 + 发牌机关字母 + 序号，7/8 位，只认大写）
    plate_cn => wheel_rs::regex_utils::is_plate_cn, "车牌号"
);
check_fn!(
    /// `http://` / `https://` 开头的绝对 URL
    url_http => wheel_rs::regex_utils::is_url_http, "URL"
);
check_fn!(
    /// 树编码（每层固定 3 位数字，1~10 层）
    tree_code => wheel_rs::regex_utils::is_tree_code, "树编码"
);
check_fn!(
    /// UUID（8-4-4-4-12，忽略大小写）
    uuid => wheel_rs::regex_utils::is_uuid, "UUID"
);
check_fn!(
    /// 登录名（字母开头，字母/数字/下划线，4~32 位）
    account_name => wheel_rs::regex_utils::is_account_name, "登录名"
);
check_fn!(
    /// 通用编码（字母/数字/`_`/`.`/`-`，1~50 位）
    code => wheel_rs::regex_utils::is_code, "编码"
);
check_fn!(
    /// 中文姓名（汉字与间隔号 `·`，2~30 字）
    chinese_name => wheel_rs::regex_utils::is_chinese_name, "姓名"
);
check_fn!(
    /// 口令（8~64 位可打印 ASCII，同时含字母与数字）
    password => wheel_rs::regex_utils::is_password, "口令"
);
check_fn!(
    /// IPv4 或 IPv6 地址（用 `std::net` 解析，比正则严）
    ip => wheel_rs::regex_utils::is_ip, "IP 地址"
);
check_fn!(
    /// IPv4 地址
    ipv4 => wheel_rs::regex_utils::is_ipv4, "IPv4 地址"
);
check_fn!(
    /// IPv6 地址
    ipv6 => wheel_rs::regex_utils::is_ipv6, "IPv6 地址"
);

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
    fn message_is_chinese() {
        let err = email("bad").unwrap_err();
        assert_eq!(err.message.as_deref(), Some("邮箱格式不正确"));
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
