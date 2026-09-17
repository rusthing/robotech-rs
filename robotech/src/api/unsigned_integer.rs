use sea_orm::sea_query::{ArrayType, Nullable, ValueType, ValueTypeErr};
use sea_orm::{ColIdx, ColumnType, QueryResult, TryGetError, TryGetable, Value};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::ToSchema;

/// # 无符号 8 位整数包装类型
///
/// 用于表示数据库中的无符号小整数（如状态、标记等），
/// 序列化时输出为裸数字而非嵌套对象；支持与 `u8` / `i8` 互转，
/// 并实现 `Deref` 以便直接作为 `u8` 使用。
#[derive(
    ToSchema,
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
)]
#[serde(transparent)] // 序列化成裸数字,而不是 {"0": 123} 这种嵌套对象
pub struct U8(pub u8);

impl U8 {
    /// 返回包装的 `u8` 值
    pub fn value(&self) -> u8 {
        self.0
    }
    pub fn max() -> Self {
        Self(u8::MAX)
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

/// # 无符号 16 位整数包装类型
///
/// 用于表示数据库中的无符号整数，序列化时输出为裸数字而非嵌套对象；
/// 支持与 `u16` / `i16` 互转，并实现 `Deref` 以便直接作为 `u16` 使用。
#[derive(
    ToSchema,
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
)]
#[serde(transparent)] // 序列化成裸数字,而不是 {"0": 123} 这种嵌套对象
pub struct U16(pub u16);

impl U16 {
    /// 返回包装的 `u16` 值
    pub fn value(&self) -> u16 {
        self.0
    }
    pub fn max() -> Self {
        Self(u16::MAX)
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

/// # 无符号 32 位整数包装类型
///
/// 用于表示数据库中的无符号整数，序列化时输出为裸数字而非嵌套对象；
/// 支持与 `u32` / `i32` 互转，并实现 `Deref` 以便直接作为 `u32` 使用。
#[derive(
    ToSchema,
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
)]
#[serde(transparent)] // 序列化成裸数字,而不是 {"0": 123} 这种嵌套对象
pub struct U32(pub u32);

impl U32 {
    /// 返回包装的 `u32` 值
    pub fn value(&self) -> u32 {
        self.0
    }
    pub fn max() -> Self {
        Self(u32::MAX)
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

/// # 无符号 64 位整数包装类型
///
/// 用于表示数据库中的无符号大整数（如主键 ID），
/// 序列化时输出为字符串以避免 JavaScript 精度丢失；
/// 支持与 `u64` / `i64` 互转，并实现 `Deref` 以及 SeaORM 的取值/绑定能力。
#[derive(ToSchema, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct U64(pub u64);

impl U64 {
    /// 返回包装的 `u64` 值
    pub fn value(&self) -> u64 {
        self.0
    }

    pub fn max() -> Self {
        Self(u64::MAX)
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

/// # 无符号 128 位整数包装类型
///
/// 用于表示数据库中的超大无符号整数，
/// 序列化时输出为字符串以避免 JavaScript 精度丢失；
/// 支持与 `u128` / `i128` 互转，并实现 `Deref` 以便直接作为 `u128` 使用。
#[derive(ToSchema, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct U128(pub u128);

impl U128 {
    /// 返回包装的 `u128` 值
    pub fn value(&self) -> u128 {
        self.0
    }
    pub fn max() -> Self {
        Self(u128::MAX)
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

macro_rules! impl_cmp {
    ($U:ident, $u:ty, $i:ty) => {
        impl PartialEq<$u> for $U {
            fn eq(&self, other: &$u) -> bool {
                self.0.eq(other)
            }
        }
        impl PartialEq<$i> for $U {
            fn eq(&self, other: &$i) -> bool {
                *other >= 0 && self.0 == *other as $u
            }
        }
        impl PartialOrd<$u> for $U {
            fn partial_cmp(&self, other: &$u) -> Option<std::cmp::Ordering> {
                self.0.partial_cmp(other)
            }
        }
        impl PartialOrd<$i> for $U {
            fn partial_cmp(&self, other: &$i) -> Option<std::cmp::Ordering> {
                if *other < 0 {
                    Some(std::cmp::Ordering::Greater)
                } else {
                    self.0.partial_cmp(&(*other as $u))
                }
            }
        }
    };
}

impl_cmp!(U8, u8, i8);
impl_cmp!(U16, u16, i16);
impl_cmp!(U32, u32, i32);
impl_cmp!(U64, u64, i64);
impl_cmp!(U128, u128, i128);

macro_rules! impl_ops {
    ($U:ident, $u:ty, $i:ty) => {
        // ========= 算术运算 =========

        // U + U -> U
        impl std::ops::Add for $U {
            type Output = $U;
            fn add(self, rhs: Self) -> Self::Output {
                $U(self.0 + rhs.0)
            }
        }
        impl std::ops::Add<$u> for $U {
            type Output = $U;
            fn add(self, rhs: $u) -> Self::Output {
                $U(self.0 + rhs)
            }
        }
        impl std::ops::Add<$U> for $u {
            type Output = $U;
            fn add(self, rhs: $U) -> Self::Output {
                $U(self + rhs.0)
            }
        }

        // U - U -> U
        impl std::ops::Sub for $U {
            type Output = $U;
            fn sub(self, rhs: Self) -> Self::Output {
                $U(self.0 - rhs.0)
            }
        }
        impl std::ops::Sub<$u> for $U {
            type Output = $U;
            fn sub(self, rhs: $u) -> Self::Output {
                $U(self.0 - rhs)
            }
        }
        impl std::ops::Sub<$U> for $u {
            type Output = $U;
            fn sub(self, rhs: $U) -> Self::Output {
                $U(self - rhs.0)
            }
        }

        // U * U -> U
        impl std::ops::Mul for $U {
            type Output = $U;
            fn mul(self, rhs: Self) -> Self::Output {
                $U(self.0 * rhs.0)
            }
        }
        impl std::ops::Mul<$u> for $U {
            type Output = $U;
            fn mul(self, rhs: $u) -> Self::Output {
                $U(self.0 * rhs)
            }
        }
        impl std::ops::Mul<$U> for $u {
            type Output = $U;
            fn mul(self, rhs: $U) -> Self::Output {
                $U(self * rhs.0)
            }
        }

        // U / U -> U
        impl std::ops::Div for $U {
            type Output = $U;
            fn div(self, rhs: Self) -> Self::Output {
                $U(self.0 / rhs.0)
            }
        }
        impl std::ops::Div<$u> for $U {
            type Output = $U;
            fn div(self, rhs: $u) -> Self::Output {
                $U(self.0 / rhs)
            }
        }
        impl std::ops::Div<$U> for $u {
            type Output = $U;
            fn div(self, rhs: $U) -> Self::Output {
                $U(self / rhs.0)
            }
        }

        // U % U -> U
        impl std::ops::Rem for $U {
            type Output = $U;
            fn rem(self, rhs: Self) -> Self::Output {
                $U(self.0 % rhs.0)
            }
        }
        impl std::ops::Rem<$u> for $U {
            type Output = $U;
            fn rem(self, rhs: $u) -> Self::Output {
                $U(self.0 % rhs)
            }
        }
        impl std::ops::Rem<$U> for $u {
            type Output = $U;
            fn rem(self, rhs: $U) -> Self::Output {
                $U(self % rhs.0)
            }
        }

        // ========= 位运算 =========

        // U & U -> U
        impl std::ops::BitAnd for $U {
            type Output = $U;
            fn bitand(self, rhs: Self) -> Self::Output {
                $U(self.0 & rhs.0)
            }
        }
        impl std::ops::BitAnd<$u> for $U {
            type Output = $U;
            fn bitand(self, rhs: $u) -> Self::Output {
                $U(self.0 & rhs)
            }
        }
        impl std::ops::BitAnd<$U> for $u {
            type Output = $U;
            fn bitand(self, rhs: $U) -> Self::Output {
                $U(self & rhs.0)
            }
        }

        // U | U -> U
        impl std::ops::BitOr for $U {
            type Output = $U;
            fn bitor(self, rhs: Self) -> Self::Output {
                $U(self.0 | rhs.0)
            }
        }
        impl std::ops::BitOr<$u> for $U {
            type Output = $U;
            fn bitor(self, rhs: $u) -> Self::Output {
                $U(self.0 | rhs)
            }
        }
        impl std::ops::BitOr<$U> for $u {
            type Output = $U;
            fn bitor(self, rhs: $U) -> Self::Output {
                $U(self | rhs.0)
            }
        }

        // U ^ U -> U
        impl std::ops::BitXor for $U {
            type Output = $U;
            fn bitxor(self, rhs: Self) -> Self::Output {
                $U(self.0 ^ rhs.0)
            }
        }
        impl std::ops::BitXor<$u> for $U {
            type Output = $U;
            fn bitxor(self, rhs: $u) -> Self::Output {
                $U(self.0 ^ rhs)
            }
        }
        impl std::ops::BitXor<$U> for $u {
            type Output = $U;
            fn bitxor(self, rhs: $U) -> Self::Output {
                $U(self ^ rhs.0)
            }
        }

        // U << u32 -> U
        impl std::ops::Shl<u32> for $U {
            type Output = $U;
            fn shl(self, rhs: u32) -> Self::Output {
                $U(self.0 << rhs)
            }
        }

        // U >> u32 -> U
        impl std::ops::Shr<u32> for $U {
            type Output = $U;
            fn shr(self, rhs: u32) -> Self::Output {
                $U(self.0 >> rhs)
            }
        }

        // !U -> U
        impl std::ops::Not for $U {
            type Output = $U;
            fn not(self) -> Self::Output {
                $U(!self.0)
            }
        }

        // ========= 带符号整数混合运算 =========

        // U + i -> U（仅当 i >= 0 时安全）
        impl std::ops::Add<$i> for $U {
            type Output = $U;
            fn add(self, rhs: $i) -> Self::Output {
                $U((self.0 as $i + rhs) as $u)
            }
        }
        impl std::ops::Add<$U> for $i {
            type Output = $U;
            fn add(self, rhs: $U) -> Self::Output {
                $U((self + rhs.0 as $i) as $u)
            }
        }

        // U - i -> U
        impl std::ops::Sub<$i> for $U {
            type Output = $U;
            fn sub(self, rhs: $i) -> Self::Output {
                $U(self.0 - rhs as $u)
            }
        }
        // i - U -> U
        impl std::ops::Sub<$U> for $i {
            type Output = $U;
            fn sub(self, rhs: $U) -> Self::Output {
                $U(self as $u - rhs.0)
            }
        }

        // U * i -> U
        impl std::ops::Mul<$i> for $U {
            type Output = $U;
            fn mul(self, rhs: $i) -> Self::Output {
                $U((self.0 as $i * rhs) as $u)
            }
        }
        impl std::ops::Mul<$U> for $i {
            type Output = $U;
            fn mul(self, rhs: $U) -> Self::Output {
                $U((self * rhs.0 as $i) as $u)
            }
        }

        // ========= 复合赋值运算 =========

        impl std::ops::AddAssign for $U {
            fn add_assign(&mut self, rhs: Self) {
                self.0 += rhs.0;
            }
        }
        impl std::ops::AddAssign<$u> for $U {
            fn add_assign(&mut self, rhs: $u) {
                self.0 += rhs;
            }
        }

        impl std::ops::SubAssign for $U {
            fn sub_assign(&mut self, rhs: Self) {
                self.0 -= rhs.0;
            }
        }
        impl std::ops::SubAssign<$u> for $U {
            fn sub_assign(&mut self, rhs: $u) {
                self.0 -= rhs;
            }
        }

        impl std::ops::MulAssign for $U {
            fn mul_assign(&mut self, rhs: Self) {
                self.0 *= rhs.0;
            }
        }
        impl std::ops::MulAssign<$u> for $U {
            fn mul_assign(&mut self, rhs: $u) {
                self.0 *= rhs;
            }
        }

        impl std::ops::DivAssign for $U {
            fn div_assign(&mut self, rhs: Self) {
                self.0 /= rhs.0;
            }
        }
        impl std::ops::DivAssign<$u> for $U {
            fn div_assign(&mut self, rhs: $u) {
                self.0 /= rhs;
            }
        }

        impl std::ops::RemAssign for $U {
            fn rem_assign(&mut self, rhs: Self) {
                self.0 %= rhs.0;
            }
        }
        impl std::ops::RemAssign<$u> for $U {
            fn rem_assign(&mut self, rhs: $u) {
                self.0 %= rhs;
            }
        }

        impl std::ops::BitAndAssign for $U {
            fn bitand_assign(&mut self, rhs: Self) {
                self.0 &= rhs.0;
            }
        }
        impl std::ops::BitAndAssign<$u> for $U {
            fn bitand_assign(&mut self, rhs: $u) {
                self.0 &= rhs;
            }
        }

        impl std::ops::BitOrAssign for $U {
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0;
            }
        }
        impl std::ops::BitOrAssign<$u> for $U {
            fn bitor_assign(&mut self, rhs: $u) {
                self.0 |= rhs;
            }
        }

        impl std::ops::BitXorAssign for $U {
            fn bitxor_assign(&mut self, rhs: Self) {
                self.0 ^= rhs.0;
            }
        }
        impl std::ops::BitXorAssign<$u> for $U {
            fn bitxor_assign(&mut self, rhs: $u) {
                self.0 ^= rhs;
            }
        }

        impl std::ops::ShlAssign<u32> for $U {
            fn shl_assign(&mut self, rhs: u32) {
                self.0 <<= rhs;
            }
        }

        impl std::ops::ShrAssign<u32> for $U {
            fn shr_assign(&mut self, rhs: u32) {
                self.0 >>= rhs;
            }
        }
    };
}

impl_ops!(U8, u8, i8);
impl_ops!(U16, u16, i16);
impl_ops!(U32, u32, i32);
impl_ops!(U64, u64, i64);
impl_ops!(U128, u128, i128);

// ========= 跨大小运算（U64 <-> u128） =========

impl std::ops::Sub<u128> for U64 {
    type Output = U64;
    fn sub(self, rhs: u128) -> Self::Output {
        U64(self.0 - rhs as u64)
    }
}

// ========= From<$U> for $u（从包装类型转回原生类型） =========

macro_rules! impl_from_u {
    ($U:ident, $u:ty) => {
        impl From<$U> for $u {
            fn from(val: $U) -> Self {
                val.0
            }
        }
    };
}

impl_from_u!(U8, u8);
impl_from_u!(U16, u16);
impl_from_u!(U32, u32);
impl_from_u!(U64, u64);
impl_from_u!(U128, u128);