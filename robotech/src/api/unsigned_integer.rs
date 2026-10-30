#[cfg(feature = "orm")]
use sea_orm::sea_query::{ArrayType, Nullable, ValueType, ValueTypeErr};
#[cfg(feature = "orm")]
use sea_orm::{ColIdx, ColumnType, QueryResult, TryGetError, TryGetable, Value};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// # 无符号整数的 `Deserialize`：字符串与数字都收
///
/// 判定与范围检查都在 [`crate::api::tolerant_number`] 里（与有符号整型、浮点共用
/// 一套逻辑），这里只负责把裸值包回包装类型。要收的形态有三种：
///
/// - 查询参数 / 路径参数走 `serde_urlencoded` 与 axum 的 path deserializer，
///   值永远是字符串；
/// - `Json` 请求体里是 JSON number（`visit_u64` / `visit_i64`）；
/// - 有些 JSON 库（Python、Java）会把整数写成 `1.0`。
macro_rules! impl_unsigned_de {
    ($U:ident, $u:ty) => {
        impl<'de> Deserialize<'de> for $U {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                crate::api::tolerant_number::de::<$u, D>(deserializer).map($U)
            }
        }
    };
}

/// # 无符号 8 位整数包装类型
///
/// 用于表示数据库中的无符号小整数（如状态、标记等），
/// 序列化时输出为裸数字而非嵌套对象；反序列化同时接受数字与字符串
/// （查询参数是字符串，JSON body 里是数字）；支持与 `u8` / `i8` 互转，
/// 并实现 `Deref` 以便直接作为 `u8` 使用。
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)] // 序列化成裸数字,而不是 {"0": 123} 这种嵌套对象
pub struct U8(pub u8);

// 入参：字符串与数字都收（见 impl_unsigned_de 的说明）
impl_unsigned_de!(U8, u8);

impl U8 {
    /// 返回包装的 `u8` 值
    pub fn value(&self) -> u8 {
        self.0
    }
    /// 返回 `u8` 的最大值
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

// ========= SeaORM四件套 =========
#[cfg(feature = "orm")]
impl TryFrom<U8> for Value {
    type Error = std::num::TryFromIntError;
    fn try_from(id: U8) -> Result<Self, Self::Error> {
        Ok(Value::TinyInt(Some(id.0 as i8)))
    }
}

#[cfg(feature = "orm")]
impl TryGetable for U8 {
    fn try_get_by<I: ColIdx>(res: &QueryResult, idx: I) -> Result<Self, TryGetError> {
        {
            let v: i8 = <i8 as TryGetable>::try_get_by(res, idx)?;
            Ok(Self(v as u8))
        }
    }
}

#[cfg(feature = "orm")]
impl ValueType for U8 {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::TinyInt(Some(x)) => Ok(Self(x as u8)),
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "U8".to_string()
    }

    fn array_type() -> ArrayType {
        ArrayType::TinyInt
    }

    fn column_type() -> ColumnType {
        ColumnType::TinyInteger
    }
}

#[cfg(feature = "orm")]
impl Nullable for U8 {
    fn null() -> Value {
        Value::TinyInt(None)
    }
}

/// # 无符号 16 位整数包装类型
///
/// 用于表示数据库中的无符号整数，序列化时输出为裸数字而非嵌套对象；
/// 反序列化同时接受数字与字符串（查询参数是字符串，JSON body 里是数字）；
/// 支持与 `u16` / `i16` 互转，并实现 `Deref` 以便直接作为 `u16` 使用。
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)] // 序列化成裸数字,而不是 {"0": 123} 这种嵌套对象
pub struct U16(pub u16);

// 入参：字符串与数字都收（见 impl_unsigned_de 的说明）
impl_unsigned_de!(U16, u16);

impl U16 {
    /// 返回包装的 `u16` 值
    pub fn value(&self) -> u16 {
        self.0
    }
    /// 返回 `u16` 的最大值
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

// ========= SeaORM四件套 =========
#[cfg(feature = "orm")]
impl TryFrom<U16> for Value {
    type Error = std::num::TryFromIntError;
    fn try_from(id: U16) -> Result<Self, Self::Error> {
        Ok(Value::SmallInt(Some(id.0 as i16)))
    }
}

#[cfg(feature = "orm")]
impl TryGetable for U16 {
    fn try_get_by<I: ColIdx>(res: &QueryResult, idx: I) -> Result<Self, TryGetError> {
        {
            let v: i16 = <i16 as TryGetable>::try_get_by(res, idx)?;
            Ok(Self(v as u16))
        }
    }
}

#[cfg(feature = "orm")]
impl ValueType for U16 {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::SmallInt(Some(x)) => Ok(Self(x as u16)),
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "U16".to_string()
    }

    fn array_type() -> ArrayType {
        ArrayType::SmallInt
    }

    fn column_type() -> ColumnType {
        ColumnType::SmallInteger
    }
}

#[cfg(feature = "orm")]
impl Nullable for U16 {
    fn null() -> Value {
        Value::SmallInt(None)
    }
}

/// # 无符号 32 位整数包装类型
///
/// 用于表示数据库中的无符号整数（如计数、时长毫秒数），
/// 序列化时输出为裸数字而非嵌套对象；反序列化同时接受数字与字符串
/// （查询参数是字符串，JSON body 里是数字）；
/// 支持与 `u32` / `i32` 互转，并实现 `Deref` 以及 SeaORM 的取值/绑定能力。
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)] // 序列化成裸数字,而不是 {"0": 123} 这种嵌套对象
pub struct U32(pub u32);

// 入参：字符串与数字都收（见 impl_unsigned_de 的说明）
impl_unsigned_de!(U32, u32);

impl U32 {
    /// 返回包装的 `u32` 值
    pub fn value(&self) -> u32 {
        self.0
    }
    /// 返回 `u32` 的最大值
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

// U32 -> u64: 无符号扩宽,总是安全,用于与 u64 时间戳之类的量做混合运算
impl From<U32> for u64 {
    fn from(v: U32) -> Self {
        v.0 as u64
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

// ========= SeaORM四件套 =========
#[cfg(feature = "orm")]
impl TryFrom<U32> for Value {
    type Error = std::num::TryFromIntError;
    fn try_from(id: U32) -> Result<Self, Self::Error> {
        Ok(Value::Int(Some(id.0 as i32)))
    }
}

#[cfg(feature = "orm")]
impl TryGetable for U32 {
    fn try_get_by<I: ColIdx>(res: &QueryResult, idx: I) -> Result<Self, TryGetError> {
        {
            let v: i32 = <i32 as TryGetable>::try_get_by(res, idx)?;
            Ok(Self(v as u32))
        }
    }
}

#[cfg(feature = "orm")]
impl ValueType for U32 {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::Int(Some(x)) => Ok(Self(x as u32)),
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "U32".to_string()
    }

    fn array_type() -> ArrayType {
        ArrayType::Int
    }

    fn column_type() -> ColumnType {
        ColumnType::Integer
    }
}

#[cfg(feature = "orm")]
impl Nullable for U32 {
    fn null() -> Value {
        Value::Int(None)
    }
}

/// # 无符号 64 位整数包装类型
///
/// 用于表示数据库中的无符号大整数（如主键 ID），
/// 序列化时输出为字符串以避免 JavaScript 精度丢失；
/// 反序列化同时接受字符串与数字（查询参数是字符串，JSON body 里可能是数字）；
/// 支持与 `u64` / `i64` 互转，并实现 `Deref` 以及 SeaORM 的取值/绑定能力。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct U64(pub u64);

impl U64 {
    /// 返回包装的 `u64` 值
    pub fn value(&self) -> u64 {
        self.0
    }

    /// 返回 `u64` 的最大值
    pub fn max() -> Self {
        Self(u64::MAX)
    }

    /// # 查询条件用的绑定值（PG `int8`）
    ///
    /// 与写路径（`From<U64> for i64`）和读路径（`TryGetable for U64`）保持同一套
    /// 位模式：`u64 as i64` 在 `2^64` 上是一一映射，`int8` 就是装 8 个字节的容器，
    /// 超过 `i64::MAX` 的值会以负数形式落库，读回来 `as u64` 仍原样还原。
    ///
    /// **三条路径必须一致**：`int8` 是有符号的，若条件里把超范围值折成 `NULL`，
    /// 就会出现「写进去一个负数行、按同一个 u64 查却查不到」——幂等键永不命中，
    /// 重复上报被当成新事件，且不报错。所以这里**不做范围裁剪**。
    ///
    /// 裸 `u64` 不能直接交给 sea-query：它会被当成 `Value::BigUnsigned`，而
    /// sea-query-sqlx 绑参时对它做 `i64::try_from(..).unwrap()`（`sqlx_postgres.rs`），
    /// 一个 `?businessId=18446744073709551615` 就能把请求任务 panic 掉、连响应都没有。
    ///
    /// 注意代价：`int8` 的有符号序会让负数排在正数之前，`ORDER BY` / `> <` /
    /// 游标分页在含负数的列上都不成立（等值匹配不受影响）。什么时候真要在意：
    /// 这一列要当游标锚点或做范围扫描时。装哈希这类“本来就没有序”的业务 ID
    /// 则无所谓，不必为此把 64 位砍成 63 位——那只会让生成端多记一条约束。
    #[cfg(feature = "orm")]
    pub fn to_db_value(&self) -> Value {
        Value::BigInt(Some(self.0 as i64))
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

// Entity(i32) -> U64:如果不会为负,单向转换总是安全的
impl From<i32> for U64 {
    fn from(v: i32) -> Self {
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

// 入参：字符串（查询参数）与数字（JSON body）都收
impl_unsigned_de!(U64, u64);

// ========= SeaORM四件套 =========
#[cfg(feature = "orm")]
impl TryFrom<U64> for Value {
    type Error = std::num::TryFromIntError;
    fn try_from(id: U64) -> Result<Self, Self::Error> {
        Ok(Value::BigInt(Some(id.0 as i64)))
    }
}

#[cfg(feature = "orm")]
impl TryGetable for U64 {
    fn try_get_by<I: ColIdx>(res: &QueryResult, idx: I) -> Result<Self, TryGetError> {
        {
            let v: i64 = <i64 as TryGetable>::try_get_by(res, idx)?;
            Ok(Self(v as u64))
        }
    }
}

#[cfg(feature = "orm")]
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

#[cfg(feature = "orm")]
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
/// 反序列化同时接受字符串与数字（查询参数是字符串，JSON body 里可能是数字）；
/// 支持与 `u128` / `i128` 互转，并实现 `Deref` 以便直接作为 `u128` 使用。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct U128(pub u128);

impl U128 {
    /// 返回包装的 `u128` 值
    pub fn value(&self) -> u128 {
        self.0
    }
    /// 返回 `u128` 的最大值
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

// 入参：字符串（查询参数）与数字（JSON body）都收
impl_unsigned_de!(U128, u128);

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

// ========= SeaORM四件套 =========
#[cfg(feature = "orm")]
impl TryFrom<U128> for Value {
    type Error = std::num::TryFromIntError;
    fn try_from(id: U128) -> Result<Self, Self::Error> {
        Ok(Value::BigInt(Some(id.0 as i64)))
    }
}

#[cfg(feature = "orm")]
impl TryGetable for U128 {
    fn try_get_by<I: ColIdx>(res: &QueryResult, idx: I) -> Result<Self, TryGetError> {
        {
            let v: i64 = <i64 as TryGetable>::try_get_by(res, idx)?;
            Ok(Self(v as u128))
        }
    }
}

#[cfg(feature = "orm")]
impl ValueType for U128 {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::BigInt(Some(x)) => Ok(Self(x as u128)),
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "U128".to_string()
    }

    fn array_type() -> ArrayType {
        ArrayType::BigInt
    }

    fn column_type() -> ColumnType {
        ColumnType::BigInteger
    }
}

#[cfg(feature = "orm")]
impl Nullable for U128 {
    fn null() -> Value {
        Value::BigInt(None)
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
// ========= validator 支持 =========
//
// `#[crud_dto]` 会把 DTO 里的 `u8..u128` 包装成 U8..U128，而 validator 只为
// 原生整数和 `Option<原生整数>` 实现了 `ValidateRange`，包装类型没有实现时，
// 字段上写 `@range 1..=2` 就编译不过。
//
// 这里按包装类型补齐：比较走内层原生值，`Option<包装>` 只在有值时比较
// （与 validator 对 `Option<u16>` 的语义一致）。区间端点也必须是包装类型，
// 所以宏生成的是 `range(min = U16::from(1), max = U16::from(2))` 这种形式。
//
// 注：validator 的 `ValidateRangeType` 标记 trait 没有公开导出（`validation`
// 模块是私有的），没法走「实现标记 trait + 复用 blanket impl」那条路，
// 只能直接实现 `ValidateRange`。
#[cfg(feature = "app")]
macro_rules! impl_validate_range {
    ($U:ident, $u:ty) => {
        impl ::validator::ValidateRange<$U> for $U {
            fn greater_than(&self, max: $U) -> Option<bool> {
                Some(self.0 > max.0)
            }

            fn less_than(&self, min: $U) -> Option<bool> {
                Some(self.0 < min.0)
            }
        }

        // 只能以包装类型作区间端点：`Option<$U>` 不在本 crate，
        // 按孤儿规则「trait 参数里得先出现本地类型」必须写成 `ValidateRange<$U>`
        impl ::validator::ValidateRange<$U> for Option<$U> {
            fn greater_than(&self, max: $U) -> Option<bool> {
                self.map(|v| v.0 > max.0)
            }

            fn less_than(&self, min: $U) -> Option<bool> {
                self.map(|v| v.0 < min.0)
            }
        }

        // `#[crud_dto]` 生成的 AddDto 把可空列写成 `Option<Option<T>>`：
        // 外层区分「字段没传」，内层是列上的 NULL
        impl ::validator::ValidateRange<$U> for Option<Option<$U>> {
            fn greater_than(&self, max: $U) -> Option<bool> {
                self.flatten().map(|v| v.0 > max.0)
            }

            fn less_than(&self, min: $U) -> Option<bool> {
                self.flatten().map(|v| v.0 < min.0)
            }
        }
    };
}

#[cfg(feature = "app")]
impl_validate_range!(U8, u8);
#[cfg(feature = "app")]
impl_validate_range!(U16, u16);
#[cfg(feature = "app")]
impl_validate_range!(U32, u32);
#[cfg(feature = "app")]
impl_validate_range!(U64, u64);
#[cfg(feature = "app")]
impl_validate_range!(U128, u128);

// ========= OpenAPI schema：字符串与数字并存的大整数 =========
//
// `U64` / `U128` 的响应是字符串（避免 JS 精度丢失），入参则是字符串和数字都收，
// 所以不能像 `U8..U32` 那样直接 derive 成 `integer`——那样文档与实现不一致，
// 客户端照着 schema 生成的模型也会把响应当数字。这里标成 `oneOf`：
// `string` 对应响应与查询参数，`integer` 对应 JSON body 里直接传数字的写法。
#[cfg(feature = "openapi")]
macro_rules! impl_string_or_integer_schema {
    ($($U:ident),* $(,)?) => {$(
        impl utoipa::PartialSchema for $U {
            fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
                use utoipa::openapi::schema::{Object, OneOf, Schema, SchemaType, Type};

                Schema::OneOf(OneOf {
                    items: vec![
                        Object::with_type(SchemaType::new(Type::String)).into(),
                        Object::with_type(SchemaType::new(Type::Integer)).into(),
                    ],
                    ..Default::default()
                })
                .into()
            }
        }

        impl utoipa::ToSchema for $U {
            fn name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed(stringify!($U))
            }
        }
    )*};
}

#[cfg(feature = "openapi")]
impl_string_or_integer_schema!(U64, U128);

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct IdBody {
        id: U64,
    }

    /// 字符串与数字都要能进：查询参数是字符串，JSON body 里前端可能直接传数字
    #[test]
    fn u64_accepts_string_and_number() {
        assert_eq!(serde_json::from_str::<U64>("\"123\"").unwrap(), U64(123));
        assert_eq!(serde_json::from_str::<U64>("123").unwrap(), U64(123));
        // 查询参数带空白（`?id=%20123`）
        assert_eq!(serde_json::from_str::<U64>("\" 123 \"").unwrap(), U64(123));

        // 边界值：u64::MAX 两种形态都能进
        assert_eq!(
            serde_json::from_str::<U64>(&u64::MAX.to_string()).unwrap(),
            U64(u64::MAX)
        );
        assert_eq!(
            serde_json::from_str::<U64>(&format!("\"{}\"", u64::MAX)).unwrap(),
            U64(u64::MAX)
        );

        // 请求体形态：2^53+1 已经超出 JS 安全整数，仍要精确还原
        let body: IdBody = serde_json::from_str(r#"{"id": 9007199254740993}"#).unwrap();
        assert_eq!(body.id, U64(9_007_199_254_740_993));
        let body: IdBody = serde_json::from_str(r#"{"id": "9007199254740993"}"#).unwrap();
        assert_eq!(body.id, U64(9_007_199_254_740_993));
    }

    /// 负数、空串、非数字、超上界必须报错，不能静默取 0
    #[test]
    fn u64_rejects_invalid() {
        assert!(serde_json::from_str::<U64>("-1").is_err());
        assert!(serde_json::from_str::<U64>("\"-1\"").is_err());
        assert!(serde_json::from_str::<U64>("\"\"").is_err());
        assert!(serde_json::from_str::<U64>("\"abc\"").is_err());
        // u64::MAX + 1：JSON 解析成浮点，进不了整数分支
        assert!(serde_json::from_str::<U64>("18446744073709551616").is_err());
    }

    /// # 查询条件的绑定值要与读写路径同一位模式
    ///
    /// 三处必须一致：写 `From<U64> for i64`、读 `TryGetable for U64`、查这里。
    /// 只要有一处做了范围裁剪（比如超范围折 `NULL`），就会出现「写进去查不到」：
    /// 高位的值以负数落库，条件却恒不成立，幂等键永不命中且不报错。
    ///
    /// 另外裸 u64 交给 sea-query 会变成 `BigUnsigned`，sea-query-sqlx 绑参时
    /// `i64::try_from(..).unwrap()` 直接 panic，所以必须包成 `BigInt`。
    #[cfg(feature = "orm")]
    #[test]
    fn u64_db_value_keeps_bit_pattern() {
        assert_eq!(U64(42).to_db_value(), Value::BigInt(Some(42)));
        assert_eq!(
            U64(i64::MAX as u64).to_db_value(),
            Value::BigInt(Some(i64::MAX))
        );
        // 超范围不折不裁，而是按位模式落成负数——读回来能还原成原值
        assert_eq!(
            U64(i64::MAX as u64 + 1).to_db_value(),
            Value::BigInt(Some(i64::MIN))
        );
        assert_eq!(U64::max().to_db_value(), Value::BigInt(Some(-1)));

        // 往返：写出再读回，任意 u64 都不丢位
        for v in [0, 1, 42, i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX] {
            let bound = U64(v).to_db_value();
            let Value::BigInt(Some(raw)) = bound else {
                panic!("u64 必须折成非空 BigInt，否则条件恒不成立");
            };
            assert_eq!(raw as u64, v, "位模式往返丢了信息");
        }
    }

    /// 输出仍是字符串，JS 侧不丢精度；且能原样读回
    #[test]
    fn u64_serializes_as_string() {
        assert_eq!(
            serde_json::to_string(&U64(9_007_199_254_740_993)).unwrap(),
            "\"9007199254740993\""
        );
        let s = serde_json::to_string(&U64(u64::MAX)).unwrap();
        assert_eq!(serde_json::from_str::<U64>(&s).unwrap(), U64(u64::MAX));
    }

    #[test]
    fn u128_accepts_string_and_number() {
        assert_eq!(serde_json::from_str::<U128>("42").unwrap(), U128(42));
        assert_eq!(serde_json::from_str::<U128>("\"42\"").unwrap(), U128(42));

        // 超 u64 的量只能走字符串：serde_json 会把装不进 u64/i64 的裸数字
        // 退化成 f64，精度在解析阶段就没了，u128 只能用字符串形态保证无损
        let big = u128::MAX.to_string();
        assert_eq!(
            serde_json::from_str::<U128>(&format!("\"{big}\"")).unwrap(),
            U128(u128::MAX)
        );
    }

    /// 小整型（U8/U16/U32）同样收字符串，但上界、负数、小数不能放松
    #[test]
    fn small_uints_accept_string_and_number() {
        assert_eq!(serde_json::from_str::<U8>("7").unwrap(), U8(7));
        assert_eq!(serde_json::from_str::<U8>("\"7\"").unwrap(), U8(7));
        assert_eq!(serde_json::from_str::<U16>("\"700\"").unwrap(), U16(700));
        assert_eq!(serde_json::from_str::<U32>("70000").unwrap(), U32(70000));
        assert_eq!(
            serde_json::from_str::<U32>("\"70000\"").unwrap(),
            U32(70000)
        );

        assert!(serde_json::from_str::<U8>("300").is_err());
        assert!(serde_json::from_str::<U8>("\"300\"").is_err());
        assert!(serde_json::from_str::<U8>("-1").is_err());
        assert!(serde_json::from_str::<U8>("1.5").is_err());
    }

    /// Python / Java 的 JSON 库会把整数写成 `1.0`，这种也要收
    #[test]
    fn accepts_integral_float() {
        assert_eq!(serde_json::from_str::<U8>("1.0").unwrap(), U8(1));
        assert_eq!(serde_json::from_str::<U32>("1.0").unwrap(), U32(1));
        assert_eq!(serde_json::from_str::<U32>("\"1.0\"").unwrap(), U32(1));
        assert_eq!(serde_json::from_str::<U64>("123.0").unwrap(), U64(123));

        // 小数部分不为 0 仍然报错
        assert!(serde_json::from_str::<U64>("123.5").is_err());
        assert!(serde_json::from_str::<U32>("\"1.5\"").is_err());
    }

    /// 可空字段：null 与缺失仍是 None，传值两种形态都能进
    #[test]
    fn option_u64_still_nullable() {
        #[derive(Debug, Deserialize)]
        struct Body {
            id: Option<U64>,
        }

        assert_eq!(
            serde_json::from_str::<Body>(r#"{"id":null}"#).unwrap().id,
            None
        );
        assert_eq!(
            serde_json::from_str::<Body>(r#"{"id":123}"#).unwrap().id,
            Some(U64(123))
        );
        assert_eq!(
            serde_json::from_str::<Body>(r#"{"id":"123"}"#).unwrap().id,
            Some(U64(123))
        );
    }

    /// OpenAPI schema 要同时给出 string 与 integer：响应是字符串，
    /// 查询参数是字符串，body 里也可以直接传数字
    #[cfg(feature = "openapi")]
    #[test]
    fn big_uint_schema_is_string_or_integer() {
        use utoipa::PartialSchema;

        fn types_of<S: PartialSchema>() -> Vec<String> {
            let schema = serde_json::to_value(S::schema()).unwrap();
            schema
                .get("oneOf")
                .expect("应该是 oneOf")
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item.get("type").unwrap().as_str().unwrap().to_string())
                .collect()
        }

        assert_eq!(types_of::<U64>(), ["string", "integer"]);
        assert_eq!(types_of::<U128>(), ["string", "integer"]);
    }
}
