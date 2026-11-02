//! # 展示脱敏包装类型
//!
//! 批量查询的 VO 不该把手机号、身份证号这类可定位个人的字段原样吐出去，
//! 而单条详情（编辑表单回显）又要拿全值——两条路**各一个 VO**，靠字段类型
//! 区分，而不是靠「记得调用某个函数」：列表 VO 的敏感字段用本模块的包装类型
//! （[`MaskedMobile`] 等），详情 VO 继续用 `String`。
//!
//! ## 类型即规则
//!
//! 包装类型**持有原值**，但出口全部遮蔽：
//!
//! - `Serialize`：输出 [`wheel_rs::mask_utils`] 的等长遮蔽结果（`138****8000`）；
//! - `Debug`：同样输出遮蔽结果——VO 打进日志不会把原值带出去；
//! - 不提供 `Deref` / `as_str` / `Display`：没有「顺手拿回原值」的路径，
//!   要原值只能走详情 VO 那条路。
//!
//! 入口（`From<String>` / `TryGetable` / `Deserialize`）收原值：
//! - `TryGetable` 从数据库 varchar 列读原值（列表查询直接把 VO 当
//!   `FromQueryResult`，见 dao 宏）；
//! - `Deserialize` 收任意字符串原样保存（客户端模式反序列化响应用），
//!   再序列化时一样遮蔽。
//!
//! ## 两套遮蔽规则
//!
//! 本模块只放**可展示类**：手机号、身份证号、邮箱、姓名、银行卡、IP，
//! 特征是「等长遮蔽、保留格式感」。密码 / 盐 / 令牌类**不进 VO**（进数据库的
//! 密码列压根不映射到 VO 字段），不需要 `MaskedSecret`——真要导出这类字段时
//! 用 [`wheel_rs::mask_utils::mask_secret`] 整串定宽遮蔽，别等长遮蔽（会暴露长度）。

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(feature = "orm")]
use sea_orm::{ColIdx, QueryResult, TryGetError, TryGetable};

/// 生成一个展示脱敏包装类型
///
/// * 持有原值，序列化与 Debug 输出 `$mask_fn` 的遮蔽结果
/// * `From<String>` / `Deserialize` / `TryGetable` 收原值
macro_rules! masked_type {
    ($name:ident, $mask_fn:path) => {
        #[derive(Clone, Default)]
        pub struct $name(String);

        impl From<String> for $name {
            fn from(value: String) -> Self {
                $name(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&$mask_fn(&self.0))
            }
        }

        /// Debug 打的是遮蔽结果：VO 进日志不携带原值
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), $mask_fn(&self.0))
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Ok($name(value))
            }
        }

        /// OpenAPI 里就是一个普通字符串
        #[cfg(feature = "openapi")]
        impl utoipa::PartialSchema for $name {
            fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
                utoipa::openapi::schema::Object::with_type(utoipa::openapi::schema::SchemaType::new(
                    utoipa::openapi::schema::Type::String,
                ))
                .into()
            }
        }

        #[cfg(feature = "openapi")]
        impl utoipa::ToSchema for $name {
            fn name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed(stringify!($name))
            }
        }

        /// 列表查询把 VO 直接当 `FromQueryResult`，从 varchar 列读原值
        #[cfg(feature = "orm")]
        impl TryGetable for $name {
            fn try_get_by<I: ColIdx>(res: &QueryResult, index: I) -> Result<Self, TryGetError> {
                <String as TryGetable>::try_get_by(res, index).map($name)
            }
        }
    };
}

masked_type!(MaskedMobile, wheel_rs::mask_utils::mask_mobile);
masked_type!(MaskedEmail, wheel_rs::mask_utils::mask_email);
masked_type!(MaskedIdCard, wheel_rs::mask_utils::mask_id_card);
masked_type!(MaskedName, wheel_rs::mask_utils::mask_name);
masked_type!(MaskedBankCard, wheel_rs::mask_utils::mask_bank_card);
masked_type!(MaskedIp, wheel_rs::mask_utils::mask_ip);

#[cfg(test)]
mod tests {
    use super::*;

    /// 序列化与 Debug 都只出遮蔽结果，原值没有任何出口
    #[test]
    fn serialize_and_debug_are_masked() {
        let mobile = MaskedMobile::from("13812348000".to_string());
        assert_eq!(serde_json::to_string(&mobile).unwrap(), "\"138****8000\"");
        assert_eq!(format!("{mobile:?}"), "MaskedMobile(138****8000)");

        let id_card = MaskedIdCard::from("11010119900307777X".to_string());
        assert!(
            serde_json::to_string(&id_card).unwrap().starts_with("\"1101"),
            "身份证遮蔽保留前四位: {}",
            serde_json::to_string(&id_card).unwrap()
        );
        assert!(!serde_json::to_string(&id_card).unwrap().contains("7777X"));

        let email = MaskedEmail::from("someone@example.com".to_string());
        let json = serde_json::to_string(&email).unwrap();
        assert!(!json.contains("omeone"), "{json}");
        assert!(json.contains("***"), "{json}");

        let name = MaskedName::from("张三".to_string());
        assert_eq!(serde_json::to_string(&name).unwrap(), "\"张*\"");

        let ip = MaskedIp::from("192.168.1.100".to_string());
        let json = serde_json::to_string(&ip).unwrap();
        assert!(!json.contains("1.100"), "{json}");
    }

    /// Option 包装：None 透传，Some 才脱敏（与库里的可空列对齐）
    #[test]
    fn optional_field_serializes_none() {
        let value: Option<MaskedMobile> = None;
        assert_eq!(serde_json::to_string(&value).unwrap(), "null");

        let value = Some(MaskedMobile::from("13812348000".to_string()));
        assert_eq!(serde_json::to_string(&value).unwrap(), "\"138****8000\"");
    }

    /// 反序列化收原值，再序列化仍遮蔽（客户端模式往返不还原）
    #[test]
    fn deserialize_keeps_raw_then_masks_again() {
        let value: MaskedMobile = serde_json::from_str("\"13812348000\"").unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), "\"138****8000\"");
    }

    /// 空值入口：空串不遮蔽（等长遮蔽对空串没有意义，照常输出空）
    #[test]
    fn empty_stays_empty() {
        let mobile = MaskedMobile::from(String::new());
        assert_eq!(serde_json::to_string(&mobile).unwrap(), "\"\"");
    }
}
