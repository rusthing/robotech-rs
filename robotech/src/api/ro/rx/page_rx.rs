use crate::api::U64;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use typed_builder::TypedBuilder;
use utoipa::ToSchema;

/// # 分页响应结构体
///
/// 用于封装分页查询结果，包含当前页码、记录总数与记录列表；
/// JSON 序列化时字段名使用 camelCase 风格。
///
/// ## 泛型参数
/// * `T` - 列表元素的类型，需实现 `ToSchema` 与 `Serialize`
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
