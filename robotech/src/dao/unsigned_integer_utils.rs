use sea_orm::sea_query::{ArrayType, Nullable, ValueType, ValueTypeErr};
use sea_orm::{ColIdx, ColumnType, QueryResult, TryGetError, TryGetable, Value};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::ToSchema;

#[derive(
    ToSchema, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)] // 序列化成裸数字,而不是 {"0": 123} 这种嵌套对象
pub struct U8(pub u8);

impl U8 {
    pub fn value(&self) -> u8 {
        self.0
    }
}

impl From<u8> for U8 {
    fn from(v: u8) -> Self {
        U8(v)
    }
}

// Entity(i8) -> U8:如果不会为负, 单向转换就是安全的
impl From<i8> for U8 {
    fn from(v: i8) -> Self {
        U8(v as u8)
    }
}

impl From<U8> for i8 {
    fn from(id: U8) -> Self {
        id.0 as i8
    }
}

impl std::fmt::Display for U8 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// 实现 Deref,方便直接当 u8 用(比如做算术、比较)
impl std::ops::Deref for U8 {
    type Target = u8;
    fn deref(&self) -> &u8 {
        &self.0
    }
}

#[derive(
    ToSchema, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)] // 序列化成裸数字,而不是 {"0": 123} 这种嵌套对象
pub struct U16(pub u16);

impl U16 {
    pub fn value(&self) -> u16 {
        self.0
    }
}

impl From<u16> for U16 {
    fn from(v: u16) -> Self {
        U16(v)
    }
}

// Entity(i16) -> U16:如果不会为负, 单向转换就是安全的
impl From<i16> for U16 {
    fn from(v: i16) -> Self {
        U16(v as u16)
    }
}

impl From<U16> for i16 {
    fn from(id: U16) -> Self {
        id.0 as i16
    }
}

impl std::fmt::Display for U16 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// 实现 Deref,方便直接当 u16 用(比如做算术、比较)
impl std::ops::Deref for U16 {
    type Target = u16;
    fn deref(&self) -> &u16 {
        &self.0
    }
}

#[derive(
    ToSchema, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)] // 序列化成裸数字,而不是 {"0": 123} 这种嵌套对象
pub struct U32(pub u32);

impl U32 {
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl From<u32> for U32 {
    fn from(v: u32) -> Self {
        U32(v)
    }
}

// Entity(i32) -> U32:如果不会为负, 单向转换就是安全的
impl From<i32> for U32 {
    fn from(v: i32) -> Self {
        U32(v as u32)
    }
}

impl From<U32> for i32 {
    fn from(id: U32) -> Self {
        id.0 as i32
    }
}

impl std::fmt::Display for U32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// 实现 Deref,方便直接当 u32 用(比如做算术、比较)
impl std::ops::Deref for U32 {
    type Target = u32;
    fn deref(&self) -> &u32 {
        &self.0
    }
}

#[derive(
    ToSchema, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
pub struct U64(pub u64);

impl U64 {
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for U64 {
    fn from(v: u64) -> Self {
        U64(v)
    }
}

// Entity(i64) -> U64:如果不会为负,单向转换总是安全的
impl From<i64> for U64 {
    fn from(v: i64) -> Self {
        U64(v as u64)
    }
}

impl From<U64> for i64 {
    fn from(id: U64) -> Self {
        id.0 as i64
    }
}

// ========= U64 自定义序列化：输出为字符串，避免 JS 精度丢失 =========
impl Serialize for U64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for U64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct U64Visitor;

        impl<'de> serde::de::Visitor<'de> for U64Visitor {
            type Value = u64;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                v.parse::<u64>().map_err(E::custom)
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                v.parse::<u64>().map_err(E::custom)
            }
        }

        deserializer.deserialize_any(U64Visitor).map(U64)
    }
}

// ========= SeaORM四件套 =========
impl TryFrom<U64> for Value {
    type Error = std::num::TryFromIntError;
    fn try_from(id: U64) -> Result<Self, Self::Error> {
        Ok(Value::BigInt(Some(id.0 as i64)))
    }
}

impl TryGetable for U64 {
    fn try_get_by<I: ColIdx>(res: &QueryResult, idx: I) -> Result<Self, TryGetError> {
        {
            let v: i64 = <i64 as TryGetable>::try_get_by(res, idx)?;
            Ok(Self(v as u64))
        }
    }
}

impl ValueType for U64 {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::BigInt(Some(x)) => Ok(Self(x as u64)),
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "U64".to_string()
    }

    fn array_type() -> ArrayType {
        ArrayType::BigInt
    }

    fn column_type() -> ColumnType {
        ColumnType::BigInteger
    }
}

impl Nullable for U64 {
    fn null() -> Value {
        Value::BigInt(None)
    }
}

impl std::fmt::Display for U64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// 实现 Deref,方便直接当 u64 用(比如做算术、比较)
impl std::ops::Deref for U64 {
    type Target = u64;
    fn deref(&self) -> &u64 {
        &self.0
    }
}

#[derive(
    ToSchema, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
pub struct U128(pub u128);

impl U128 {
    pub fn value(&self) -> u128 {
        self.0
    }
}

impl From<u128> for U128 {
    fn from(v: u128) -> Self {
        U128(v)
    }
}

// Entity(i128) -> U128:如果不会为负, 单向转换就是安全的
impl From<i128> for U128 {
    fn from(v: i128) -> Self {
        U128(v as u128)
    }
}

impl From<U128> for i128 {
    fn from(id: U128) -> Self {
        id.0 as i128
    }
}

// ========= U128 自定义序列化：输出为字符串，避免 JS 精度丢失 =========
impl Serialize for U128 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for U128 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct U128Visitor;

        impl<'de> serde::de::Visitor<'de> for U128Visitor {
            type Value = u128;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                v.parse::<u128>().map_err(E::custom)
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                v.parse::<u128>().map_err(E::custom)
            }
        }

        deserializer.deserialize_any(U128Visitor).map(U128)
    }
}

impl std::fmt::Display for U128 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// 实现 Deref,方便直接当 u128 用(比如做算术、比较)
impl std::ops::Deref for U128 {
    type Target = u128;
    fn deref(&self) -> &u128 {
        &self.0
    }
}