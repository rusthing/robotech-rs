use crate::dao::{init_foreign_keys, init_unique_keys, DaoError};
use crate::db::get_db_conn;
use sea_orm::sea_query::{Expr, Func};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbConn,
    ExprTrait, TransactionTrait,
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
