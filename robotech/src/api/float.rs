use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[cfg(feature = "orm")]
use sea_orm::sea_query::{ArrayType, ColumnType, Nullable, ValueType, ValueTypeErr};
#[cfg(feature = "orm")]
use sea_orm::{ColIdx, QueryResult, TryGetError, TryGetable, Value};

/// # 64 位浮点包装类型
///
/// 用于 DTO/VO 与数据库 `NUMERIC`/`DECIMAL` 列的互转。
///
/// 数据库里的小数列在实体（`mo`）上是 `Decimal`，而业务计算与接口输出要的是
/// `f64`。sea-orm 的 `TryGetable` 不做跨类型转换，VO 字段直接写 `f64` 时解码
/// `NUMERIC` 列会报「Rust type f64 is not compatible with SQL type NUMERIC」，
/// 因此需要一个包装类型把这一步接上——与 `Duration`（数据库 `String` ↔ 接口
/// `Duration`）、`U64`（数据库 `i64` ↔ 接口 `u64`）是同一类东西。
///
/// - `TryGetable`：先按 `Decimal` 读（`NUMERIC`/`DECIMAL` 列），读不到再按
///   `f64` 读（`float8`/`float4` 列），两种列都能解；
/// - `Serialize`/`Deserialize`：与 `f64` 一样是裸数字，不改变接口形态；
/// - `ToSchema`：映射为 `number/double`；
/// - `Deref` 到 `f64`，可直接参与运算。
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct F64(pub f64);

impl F64 {
    /// 返回包装的 `f64` 值
    pub fn value(&self) -> f64 {
        self.0
    }
}

impl From<f64> for F64 {
    fn from(v: f64) -> Self {
        F64(v)
    }
}

impl From<F64> for f64 {
    fn from(v: F64) -> Self {
        v.0
    }
}

/// 解引用为 `f64`，方便直接参与运算
impl Deref for F64 {
    type Target = f64;

    fn deref(&self) -> &f64 {
        &self.0
    }
}

impl std::fmt::Display for F64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// OpenAPI schema 映射为 number
#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for F64 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::Object::with_type(utoipa::openapi::schema::SchemaType::new(
            utoipa::openapi::schema::Type::Number,
        ))
        .into()
    }
}

#[cfg(feature = "openapi")]
impl utoipa::ToSchema for F64 {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("F64")
    }
}

/// 从 `NUMERIC`/`DECIMAL`（`Decimal`）或浮点列读取并转换为 `f64`
#[cfg(feature = "orm")]
impl TryGetable for F64 {
    fn try_get_by<I: ColIdx>(res: &QueryResult, index: I) -> Result<Self, TryGetError> {
        // 小数列：先按 Decimal 读，这是 VO 上写 F64 的主要场景
        if let Ok(d) = <sea_orm::prelude::Decimal as TryGetable>::try_get_by(res, index) {
            return <f64 as TryFrom<sea_orm::prelude::Decimal>>::try_from(d)
                .map(F64)
                .map_err(|e| {
                    TryGetError::DbErr(sea_orm::DbErr::Type(format!("Decimal 转 f64 失败: {e}")))
                });
        }
        // 浮点列：直接按 f64 读
        let v = <f64 as TryGetable>::try_get_by(res, index)?;
        Ok(F64(v))
    }
}

/// `Decimal` -> `F64`：VO 由实体转换（`#[vo]` 的 `from_owned(Model)`）时使用
#[cfg(feature = "orm")]
impl From<sea_orm::prelude::Decimal> for F64 {
    fn from(d: sea_orm::prelude::Decimal) -> Self {
        F64(<f64 as TryFrom<sea_orm::prelude::Decimal>>::try_from(d).unwrap_or_default())
    }
}

/// `F64` -> `Decimal`：DTO 写入小数列时使用
#[cfg(feature = "orm")]
impl From<F64> for sea_orm::prelude::Decimal {
    fn from(v: F64) -> Self {
        <sea_orm::prelude::Decimal as TryFrom<f64>>::try_from(v.0).unwrap_or_default()
    }
}

#[cfg(feature = "orm")]
impl ValueType for F64 {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::Decimal(Some(x)) => <f64 as TryFrom<sea_orm::prelude::Decimal>>::try_from(x)
                .map(F64)
                .map_err(|_| ValueTypeErr),
            Value::Double(Some(x)) => Ok(Self(x)),
            Value::Float(Some(x)) => Ok(Self(x as f64)),
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "F64".to_string()
    }

    fn array_type() -> ArrayType {
        ArrayType::Double
    }

    fn column_type() -> ColumnType {
        ColumnType::Double
    }
}

#[cfg(feature = "orm")]
impl Nullable for F64 {
    fn null() -> Value {
        Value::Double(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 接口形态必须和 `f64` 一致：裸数字，不是 `Decimal` 那样的字符串
    #[test]
    fn serializes_as_number() {
        assert_eq!(serde_json::to_string(&F64(1.5)).unwrap(), "1.5");
        assert_eq!(serde_json::to_string(&F64(-0.25)).unwrap(), "-0.25");

        let v: F64 = serde_json::from_str("109.71716349347491").unwrap();
        assert!((v.0 - 109.71716349347491_f64).abs() < f64::EPSILON);
    }

    #[test]
    fn deref_to_f64() {
        let v = F64(2.5);
        assert_eq!(*v, 2.5);
        assert_eq!(f64::from(v), 2.5);
    }
}
