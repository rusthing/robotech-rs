use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::time::Duration as StdDuration;

/// `Duration` 的包装类型，用于 DTO/VO 中与数据库 `String` 类型的互转。
///
/// 实现 `Serialize`/`Deserialize`（序列化为毫秒数）、`ToSchema`（映射为 String 类型），
/// 以及 `From<String>`/`Into<String>`（通过 `humantime` 解析/格式化）。
///
/// 通过 `Deref` 可直接当作 `std::time::Duration` 使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Duration(pub StdDuration);

impl Duration {
    pub fn from_secs(secs: u64) -> Self {
        Self(StdDuration::from_secs(secs))
    }

    pub fn from_millis(millis: u64) -> Self {
        Self(StdDuration::from_millis(millis))
    }

    pub fn as_duration(&self) -> StdDuration {
        self.0
    }
}

impl Deref for Duration {
    type Target = StdDuration;

    fn deref(&self) -> &StdDuration {
        &self.0
    }
}

impl From<StdDuration> for Duration {
    fn from(d: StdDuration) -> Self {
        Self(d)
    }
}

impl From<Duration> for StdDuration {
    fn from(d: Duration) -> Self {
        d.0
    }
}

impl From<String> for Duration {
    fn from(s: String) -> Self {
        Self(wheel_rs::time_utils::string_to_duration(s))
    }
}

impl From<Duration> for String {
    fn from(d: Duration) -> Self {
        wheel_rs::time_utils::duration_to_string(d.0)
    }
}

impl Serialize for Duration {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_millis().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Duration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let millis = u64::deserialize(deserializer)?;
        Ok(Self(StdDuration::from_millis(millis)))
    }
}

impl utoipa::PartialSchema for Duration {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::Object::with_type(utoipa::openapi::schema::SchemaType::new(
            utoipa::openapi::schema::Type::String,
        ))
        .into()
    }
}

impl utoipa::ToSchema for Duration {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Duration")
    }
}

#[cfg(feature = "db")]
impl sea_orm::TryGetable for Duration {
    fn try_get_by<I: sea_orm::ColIdx>(
        res: &sea_orm::QueryResult,
        index: I,
    ) -> Result<Self, sea_orm::TryGetError> {
        let s: String = sea_orm::TryGetable::try_get_by(res, index)?;
        Ok(Duration::from(s))
    }
}