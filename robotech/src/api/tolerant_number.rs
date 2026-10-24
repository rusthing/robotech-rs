//! # 数值字段的宽容反序列化：字符串与数字都收
//!
//! 为什么需要：同一份 DTO，入参形态却不止一种——
//!
//! - 查询参数 / 路径参数（`Query` / `Path`）永远是字符串；
//! - `Json` 请求体里是 JSON number；
//! - Python、Java 的 JSON 库还会把整数写成 `1.0`。
//!
//! 原生 `i64` / `f64` 的 `Deserialize` 只认数字，客户端换个传法就 400。
//!
//! 包装类型（`U8..U128` / `F64`）自己已经是宽容的（见 `unsigned_integer.rs`、
//! `float.rs`），但有符号整型和小数没有对应的包装类型——它们在 DTO 里就是原生
//! `i8..i128` / `f32` / `f64`。给它们加包装类型会把 `#[crud_dto]` 生成的字段从
//! `Option<i64>` 改成 `Option<I64>`，等于让所有下游代码跟着改类型。所以这里走
//! serde 的 `deserialize_with`：**字段类型不变，只放宽反序列化**。
//!
//! `#[crud_dto]` 会给数值字段自动挂上（见 `robotech-macros/src/dto/mod.rs` 的
//! `generate_serde_attr`），手写 DTO 也可以照下面抄：
//!
//! | 字段类型 | 挂哪个 |
//! | --- | --- |
//! | `i64` / `f64` | [`de`] |
//! | `Option<i64>` / `Option<f64>` | [`de_opt`] |
//! | `Option<Option<i64>>` | [`de_opt_opt`] |
//!
//! `null` 的语义保持 serde 默认：`Option<T>` 收到 `null` 是 `None`，
//! `Option<Option<T>>` 收到 `null` 是外层 `None`（= 字段没传，部分更新时不改列）。
//! 宽容只作用在「有值」那一侧，不改变 modify 语义。

use serde::Deserializer;
use serde::de::Visitor;
use std::fmt;
use std::marker::PhantomData;

/// # 能从字符串和各种数字形态无损转过来的数值类型
///
/// 只给整数与浮点实现（见文件末尾的宏），不打算开放给业务类型：
/// 每个方法返回 `Result<_, String>` 而不是 `serde::de::Error`，是为了让 visitor
/// 与具体错误类型解耦（`deserialize_with` 的 `D::Error` 由调用侧决定）。
pub trait TolerantNumber: Sized {
    /// 字符串形态：`"123"`、`"1.5"`、`" 123 "`（调用侧已 trim）
    fn parse_str(s: &str) -> Result<Self, String>;
    /// JSON 正整数 / 无符号数
    fn from_u64(v: u64) -> Result<Self, String>;
    /// JSON 负整数
    fn from_i64(v: i64) -> Result<Self, String>;
    /// JSON 里的超大整数（serde_json 一般会退化成 `f64`，留着以防别的格式）
    fn from_u128(v: u128) -> Result<Self, String>;
    /// 同上，带符号
    fn from_i128(v: i128) -> Result<Self, String>;
    /// JSON 的 `1.0`
    fn from_f64(v: f64) -> Result<Self, String>;
    /// 类型名，用于报错信息
    fn type_name() -> &'static str;
}

/// 单个数值的 visitor
struct NumVisitor<T>(PhantomData<T>);

impl<'de, T: TolerantNumber> Visitor<'de> for NumVisitor<T> {
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}（数字或字符串）", T::type_name())
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        let v = v.trim();
        // 先按目标类型解析，不经过浮点：`i64::MIN` 这类边界值才精确；
        // 失败再按浮点试一次，兜住 `"1.0"` 这种写法
        match T::parse_str(v) {
            Ok(n) => Ok(n),
            Err(e) => match v.parse::<f64>() {
                Ok(f) => T::from_f64(f).map_err(E::custom),
                Err(_) => Err(E::custom(e)),
            },
        }
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
        T::from_u64(v).map_err(E::custom)
    }

    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
        T::from_i64(v).map_err(E::custom)
    }

    fn visit_u128<E: serde::de::Error>(self, v: u128) -> Result<Self::Value, E> {
        T::from_u128(v).map_err(E::custom)
    }

    fn visit_i128<E: serde::de::Error>(self, v: i128) -> Result<Self::Value, E> {
        T::from_i128(v).map_err(E::custom)
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
        T::from_f64(v).map_err(E::custom)
    }
}

/// 可空数值的 visitor：`null` 仍是 `None`
struct OptVisitor<T>(PhantomData<T>);

impl<'de, T: TolerantNumber> Visitor<'de> for OptVisitor<T> {
    type Value = Option<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "null 或 {}", T::type_name())
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        de::<T, D>(deserializer).map(Some)
    }
}

/// 反序列化单个数值：`i64`、`f64` 这类非可空字段用这个
pub fn de<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: TolerantNumber,
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(NumVisitor::<T>(PhantomData))
}

/// 反序列化可空数值：`Option<i64>` 字段用这个（`null` 仍是 `None`）
pub fn de_opt<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: TolerantNumber,
    D: Deserializer<'de>,
{
    deserializer.deserialize_option(OptVisitor::<T>(PhantomData))
}

/// 反序列化双层可空数值：`#[crud_dto]` 把 `Option<i64>` 列扩成的
/// `Option<Option<i64>>` 字段用这个
///
/// 语义与 serde 对 `Option<Option<T>>` 的默认实现一致：`null` 是外层 `None`
/// （= 字段没传），有值才是 `Some(Some(v))`；`Some(None)` 不会由反序列化产生。
pub fn de_opt_opt<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: TolerantNumber,
    D: Deserializer<'de>,
{
    Ok(de_opt::<T, D>(deserializer)?.map(Some))
}

/// 浮点 -> 整数的落库前检查：整数值、在 `[min, max]` 开区间内
///
/// 上界必须是精确的 2 的幂（`2^(BITS-1)` / `2^BITS`），不能用
/// `T::MAX as f64`——后者对大整型会向上取整到 2^BITS，边界值混过去。
macro_rules! integral {
    ($v:expr, $upper:expr, $lower:expr) => {{
        let v: f64 = $v;
        let upper: f64 = $upper;
        let lower: f64 = $lower;
        if v.is_finite() && v.fract() == 0.0 && v >= lower && v < upper {
            Ok(v)
        } else {
            Err(format!("{v} 不是该整数类型可表示的值"))
        }
    }};
}

macro_rules! impl_unsigned_number {
    ($($t:ty),* $(,)?) => {$(
        impl TolerantNumber for $t {
            fn parse_str(s: &str) -> Result<Self, String> {
                s.parse::<$t>().map_err(|e: std::num::ParseIntError| e.to_string())
            }

            fn from_u64(v: u64) -> Result<Self, String> {
                <$t>::try_from(v).map_err(|e| e.to_string())
            }

            fn from_i64(v: i64) -> Result<Self, String> {
                <$t>::try_from(v).map_err(|e| e.to_string())
            }

            fn from_u128(v: u128) -> Result<Self, String> {
                <$t>::try_from(v).map_err(|e| e.to_string())
            }

            fn from_i128(v: i128) -> Result<Self, String> {
                <$t>::try_from(v).map_err(|e| e.to_string())
            }

            fn from_f64(v: f64) -> Result<Self, String> {
                integral!(v, 2.0f64.powi(<$t>::BITS as i32), 0.0).map(|v| v as $t)
            }

            fn type_name() -> &'static str {
                stringify!($t)
            }
        }
    )*};
}

macro_rules! impl_signed_number {
    ($($t:ty),* $(,)?) => {$(
        impl TolerantNumber for $t {
            fn parse_str(s: &str) -> Result<Self, String> {
                s.parse::<$t>().map_err(|e: std::num::ParseIntError| e.to_string())
            }

            fn from_u64(v: u64) -> Result<Self, String> {
                <$t>::try_from(v).map_err(|e| e.to_string())
            }

            fn from_i64(v: i64) -> Result<Self, String> {
                <$t>::try_from(v).map_err(|e| e.to_string())
            }

            fn from_u128(v: u128) -> Result<Self, String> {
                <$t>::try_from(v).map_err(|e| e.to_string())
            }

            fn from_i128(v: i128) -> Result<Self, String> {
                <$t>::try_from(v).map_err(|e| e.to_string())
            }

            fn from_f64(v: f64) -> Result<Self, String> {
                // i8 的区间是 [-128, 128)，端点用 2 的幂精确表达
                let bound = 2.0f64.powi(<$t>::BITS as i32 - 1);
                integral!(v, bound, -bound).map(|v| v as $t)
            }

            fn type_name() -> &'static str {
                stringify!($t)
            }
        }
    )*};
}

impl_unsigned_number!(u8, u16, u32, u64, u128);
impl_signed_number!(i8, i16, i32, i64, i128);

impl TolerantNumber for f32 {
    fn parse_str(s: &str) -> Result<Self, String> {
        let v: f64 = s
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string())?;
        Self::from_f64(v)
    }

    fn from_u64(v: u64) -> Result<Self, String> {
        Ok(v as f32)
    }

    fn from_i64(v: i64) -> Result<Self, String> {
        Ok(v as f32)
    }

    fn from_u128(v: u128) -> Result<Self, String> {
        Ok(v as f32)
    }

    fn from_i128(v: i128) -> Result<Self, String> {
        Ok(v as f32)
    }

    fn from_f64(v: f64) -> Result<Self, String> {
        // JSON 表达不了 NaN/inf，字符串形态（`"NaN"`、`"1e39"`）也一并挡住：
        // 前者是脏值，后者转 f32 会溢出成 inf，写进库没有意义
        if !v.is_finite() {
            return Err(format!("{v} 不是有限浮点数"));
        }
        let v = v as f32;
        if v.is_finite() {
            Ok(v)
        } else {
            Err(format!("{v} 超出 f32 范围"))
        }
    }

    fn type_name() -> &'static str {
        "f32"
    }
}

impl TolerantNumber for f64 {
    fn parse_str(s: &str) -> Result<Self, String> {
        let v: f64 = s
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string())?;
        Self::from_f64(v)
    }

    fn from_u64(v: u64) -> Result<Self, String> {
        Ok(v as f64)
    }

    fn from_i64(v: i64) -> Result<Self, String> {
        Ok(v as f64)
    }

    fn from_u128(v: u128) -> Result<Self, String> {
        Ok(v as f64)
    }

    fn from_i128(v: i128) -> Result<Self, String> {
        Ok(v as f64)
    }

    fn from_f64(v: f64) -> Result<Self, String> {
        // JSON 表达不了 NaN/inf，字符串形态也一并挡住，避免脏值写库
        if v.is_finite() {
            Ok(v)
        } else {
            Err(format!("{v} 不是有限浮点数"))
        }
    }

    fn type_name() -> &'static str {
        "f64"
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    /// 模拟 `#[crud_dto]` 生成的 DTO 字段形态
    #[derive(Debug, Default, Deserialize)]
    #[serde(default)]
    struct Body {
        #[serde(deserialize_with = "crate::api::tolerant_number::de_opt")]
        age: Option<i64>,
        #[serde(deserialize_with = "crate::api::tolerant_number::de_opt_opt")]
        score: Option<Option<f64>>,
        /// 无符号：负数、超上界要挡
        #[serde(deserialize_with = "crate::api::tolerant_number::de_opt")]
        level: Option<u8>,
        #[serde(deserialize_with = "crate::api::tolerant_number::de_opt")]
        ratio: Option<f32>,
    }

    fn parse(json: &str) -> Body {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn integer_accepts_number_and_string() {
        assert_eq!(parse(r#"{"age":18}"#).age, Some(18));
        assert_eq!(parse(r#"{"age":"18"}"#).age, Some(18));
        assert_eq!(parse(r#"{"age":" 18 "}"#).age, Some(18));
        // Python / Java 的 `1.0`
        assert_eq!(parse(r#"{"age":18.0}"#).age, Some(18));
        assert_eq!(parse(r#"{"age":"18.0"}"#).age, Some(18));
        // 负数（有符号类型本来就该收）
        assert_eq!(parse(r#"{"age":-18}"#).age, Some(-18));
        assert_eq!(parse(r#"{"age":"-18"}"#).age, Some(-18));
    }

    #[test]
    fn integer_rejects_invalid() {
        assert!(serde_json::from_str::<Body>(r#"{"age":"abc"}"#).is_err());
        assert!(serde_json::from_str::<Body>(r#"{"age":""}"#).is_err());
        // 小数部分不为 0 不能截断成整数
        assert!(serde_json::from_str::<Body>(r#"{"age":18.5}"#).is_err());
    }

    /// 无符号：负数、超上界都报错，别静默取 0 / 饱和
    #[test]
    fn unsigned_bounds_are_enforced() {
        assert_eq!(parse(r#"{"level":"7"}"#).level, Some(7));
        assert_eq!(parse(r#"{"level":7}"#).level, Some(7));
        assert!(serde_json::from_str::<Body>(r#"{"level":-1}"#).is_err());
        assert!(serde_json::from_str::<Body>(r#"{"level":256}"#).is_err());
        assert!(serde_json::from_str::<Body>(r#"{"level":"256"}"#).is_err());
    }

    #[test]
    fn float_accepts_number_and_string() {
        assert_eq!(parse(r#"{"ratio":1.5}"#).ratio, Some(1.5));
        assert_eq!(parse(r#"{"ratio":"1.5"}"#).ratio, Some(1.5));
        assert_eq!(parse(r#"{"ratio":2}"#).ratio, Some(2.0));
        assert_eq!(parse(r#"{"ratio":"1e3"}"#).ratio, Some(1000.0));
        assert!(serde_json::from_str::<Body>(r#"{"ratio":"NaN"}"#).is_err());
        assert!(serde_json::from_str::<Body>(r#"{"ratio":"abc"}"#).is_err());
    }

    /// `null` 的语义不能变：`Option<T>` 的 null 是 `None`
    #[test]
    fn null_stays_none() {
        assert_eq!(parse(r#"{"age":null}"#).age, None);
        assert_eq!(parse("{}").age, None);
    }

    /// 双层 Option：null 是外层 None（= 字段没传），有值才 Some(Some(v))
    #[test]
    fn double_option_keeps_serde_semantics() {
        assert_eq!(parse(r#"{"score":null}"#).score, None);
        assert_eq!(parse("{}").score, None);
        assert_eq!(parse(r#"{"score":"1.5"}"#).score, Some(Some(1.5)));
        assert_eq!(parse(r#"{"score":1.5}"#).score, Some(Some(1.5)));
    }
}
