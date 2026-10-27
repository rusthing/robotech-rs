//! 测试实体：`tx_item`
//!
//! 结构刻意与业务表一致（`id` 非自增但无默认值，审计字段非空），这样 DAO 的
//! 「缺 id 用 idworker 补、缺时间戳用 now_ms 补」这两条路径也会被覆盖到。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, Default)]
#[sea_orm(table_name = "tx_item")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, unique)]
    pub id: i64,
    #[sea_orm(unique)]
    pub code: String,
    pub name: Option<String>,
    pub creater_id: i64,
    pub create_ms: i64,
    pub updater_id: i64,
    pub update_ms: i64,
}

impl ActiveModelBehavior for ActiveModel {}
