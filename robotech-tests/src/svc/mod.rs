//! 测试用的最小 `#[svc]`
//!
//! 钩子在这里只做两类事：
//!
//! 1. **写标记行**（用钩子拿到的 `db`）—— 标记是否随业务写一起回滚，是「钩子与写
//!    在同一个事务里」的唯一证据；
//! 2. **故意失败** —— 用来验证回滚真的把整片写操作都撤掉了：新增按编码 `boom`
//!    失败，修改按名称 [`NOT_FOUND`] 失败（后者还钉住 `upsert` 的分支判据）。

use crate::dto::tx_item_dto::{TxItemAddDto, TxItemModifyDto};
use crate::probe;
use robotech::macros::svc;

#[svc(
    skip: [
        save,
        del_by_query_dto,
        get_by_query_dto,
        list_by_query_dto,
        page_by_query_dto,
        get_ex_by_id,
        get_ex_by_query_dto,
        list_ex_by_query_dto,
        page_ex_by_query_dto,
        search_by_id,
        search_by_condition,
        search_list,
        search_page,
        search_ex_by_id,
        search_ex_by_condition,
        search_list_ex,
        search_page_ex,
    ],
    before_add = crate::svc::before_add,
    before_modify = crate::svc::before_modify,
    after_write = crate::svc::after_write
)]
pub struct TxItemSvc;

/// 编码为 `boom` 时让写操作失败，用于验证回滚
const BOOM: &str = "boom";

/// 名称写成它时让 `before_modify` 报 `SvcError::NotFound`
///
/// 专门钉 `upsert` 的边界：业务层的 `NotFound` **不等于**「本行不存在」。
/// 框架只认 DAO 的 `RecordNotUpdated`，所以这种失败必须原样抛给调用方，
/// 不能悄悄退化成一次插入。
const NOT_FOUND: &str = "trigger-not-found";

/// 新增前：用事务连接写标记行；`boom` 编码直接返回错误
pub async fn before_add<C: ConnectionTrait>(
    add_dto: TxItemAddDto,
    db: &C,
) -> Result<TxItemAddDto, SvcError> {
    let code = add_dto.code.clone().unwrap_or_default();
    probe::insert_marker(db, &format!("before_add:{code}")).await?;
    if code == BOOM {
        return Err(SvcError::NotFound("before_add 故意失败".to_string()));
    }
    Ok(add_dto)
}

/// 修改前：同样写标记行
///
/// 参数里没有 `code` 的语义时用 id 拼备注，测试按这个键反查是否被回滚。
/// 名称写成 [`NOT_FOUND`] 时故意报 `NotFound`（见该常量的说明）。
pub async fn before_modify<C: ConnectionTrait>(
    modify_dto: TxItemModifyDto,
    db: &C,
) -> Result<TxItemModifyDto, SvcError> {
    let id = modify_dto.id.map(|id| id.to_string()).unwrap_or_default();
    probe::insert_marker(db, &format!("before_modify:{id}")).await?;
    if modify_dto.name.as_ref().and_then(|v| v.as_deref()) == Some(NOT_FOUND) {
        return Err(SvcError::NotFound("before_modify 故意报找不到".to_string()));
    }
    Ok(modify_dto)
}

/// 写操作提交后：从**独立连接**确认这行已经可见
///
/// 这是「`after_write` 必须在提交之后触发」的直接断言点。
pub async fn after_write() {
    probe::observe_after_write().await;
}
