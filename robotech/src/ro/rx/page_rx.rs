use crate::dao::U64;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use typed_builder::TypedBuilder;
use utoipa::ToSchema;

#[skip_serializing_none]
#[derive(ToSchema, Debug, Serialize, Deserialize, Clone, Setters, TypedBuilder)]
#[builder]
#[serde(rename_all = "camelCase")]
#[serde(bound(
    serialize = "T: utoipa::ToSchema + serde::Serialize",
    deserialize = "T: serde::de::DeserializeOwned"
))]
pub struct PageRx<T>
where
    T: utoipa::ToSchema + serde::Serialize,
{
    /// 当前页码
    pub page_num: U64,
    /// 记录总数
    pub total: U64,
    /// 记录列表
    pub list: Vec<T>,
}