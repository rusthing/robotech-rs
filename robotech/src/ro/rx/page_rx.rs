use derive_setters::Setters;
use serde::Serialize;
use serde_with::skip_serializing_none;
use typed_builder::TypedBuilder;
use utoipa::ToSchema;

#[skip_serializing_none]
#[derive(ToSchema, Debug, Serialize, Clone, Setters, TypedBuilder)]
#[builder]
#[serde(rename_all = "camelCase")]
pub struct PageRx<T>
where
    T: utoipa::ToSchema + serde::Serialize,
{
    /// 当前页码
    pub page_num: u64,
    /// 记录总数
    pub total: u64,
    /// 记录列表
    pub list: Vec<T>,
}
