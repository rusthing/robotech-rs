use crate::dao::{init_foreign_keys, init_unique_keys, DaoError};
use crate::db::get_db_conn;
use anyhow::anyhow;
use sea_orm::sea_query::{Expr, Func};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbConn,
    ExprTrait, QueryOrder, TransactionTrait,
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

pub fn add_order_by<Q>(mut query: Q, order_by: &Option<String>) -> Result<Q, DaoError>
where
    Q: QueryOrder,
{
    if let Some(order_by) = order_by {
        for order_by in order_by.split(",") {
            let (col, order) = if order_by.trim().to_lowercase().ends_with(":desc") {
                (order_by.trim().replace(":desc", ""), false)
            } else {
                (order_by.trim().replace(":asc", ""), true)
            };
            let col_parts: Vec<&str> = col.split(".").collect();
            let col = if col_parts.len() == 1 {
                Expr::col(col)
            } else if col_parts.len() == 2 {
                Expr::col((col_parts[0].to_string(), col_parts[1].to_string()))
            } else {
                return Err(DaoError::from(anyhow!(format!(
                    "_order_by 参数的格式错误：{order_by}"
                ))));
            };

            if order {
                query = query.order_by_asc(col);
            } else {
                query = query.order_by_desc(col);
            }
        }
    }
    Ok(query)
}