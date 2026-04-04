use crate::dao::{DaoError, init_foreign_keys, init_unique_keys};
use crate::db::get_db_conn;
use anyhow::anyhow;
use sea_orm::sea_query::{Expr, Func};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbConn,
    EntityTrait, ExprTrait, QueryOrder, Select, TransactionTrait,
};
use std::sync::Arc;

pub fn init_dao() -> Result<(), DaoError> {
    init_unique_keys()?;
    init_foreign_keys()?;
    Ok(())
}

pub fn unwrap_db<C>(db: Option<Arc<C>>) -> Result<Arc<C>, DaoError>
where
    C: ConnectionTrait,
    Arc<C>: From<Arc<DatabaseConnection>>,
{
    if let Some(db) = db {
        Ok(db)
    } else {
        get_db_conn()
            .map_err(|_| DaoError::GetDbConn())
            .map(|conn| conn.into())
    }
}

pub async fn begin_transaction(db: &DbConn) -> Result<DatabaseTransaction, DaoError> {
    Ok(db.begin().await?)
}

pub async fn commit_transaction(db: DatabaseTransaction) -> Result<(), DaoError> {
    db.commit().await?;
    Ok(())
}

/// 关键字多字段OR模糊查询
pub fn build_like_condition<T>(keyword: &str, cols: &[T]) -> Condition
where
    T: ColumnTrait,
{
    cols.into_iter().fold(Condition::any(), |condition, col| {
        condition.add(Func::lower(Expr::col(*col)).like(format!("%{}%", keyword.to_lowercase())))
    })
}

pub fn add_order_by<E>(
    mut select: Select<E>,
    order_by: &Option<String>,
) -> Result<Select<E>, DaoError>
where
    E: EntityTrait,
{
    if let Some(order_by) = order_by {
        for order_by in order_by.split(",") {
            let order_by_split = order_by.split(":");
            let mut parts = order_by_split.clone();
            let col = parts.next().ok_or_else(|| {
                DaoError::from(anyhow!(format!("_order_by 参数的格式错误：{order_by}")))
            })?;
            let order = parts.next().unwrap_or("asc");

            let col = Expr::col(col.to_string());

            // 根据列名和排序方式添加排序条件
            if order.eq_ignore_ascii_case("asc") {
                select = select.order_by_asc(col);
            } else if order.eq_ignore_ascii_case("desc") {
                select = select.order_by_desc(col);
            } else {
                return Err(DaoError::from(anyhow!(format!(
                    "_order_by 参数的格式错误：{order_by}"
                ))));
            }
        }
    }
    Ok(select)
}
