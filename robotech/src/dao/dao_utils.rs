use crate::dao::{init_foreign_keys, init_unique_fields, DaoError};
use crate::db::get_db_conn;
use sea_orm::sea_query::{Expr, Func, SimpleExpr};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbConn,
    ExprTrait, TransactionTrait,
};
use std::sync::Arc;

pub fn init_dao() -> Result<(), DaoError> {
    init_unique_fields()?;
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

/// 单字段不区分大小写模糊查询
pub fn like<T>(keyword: &str, col: &T) -> SimpleExpr
where
    T: ColumnTrait,
{
    Func::lower(Expr::col(*col)).like(format!("%{}%", keyword.to_lowercase()))
}

/// 多字段 OR 模糊查询
pub fn like_any<T>(keyword: &str, cols: &Vec<T>) -> Condition
where
    T: ColumnTrait,
{
    cols.into_iter()
        .fold(Condition::any(), |cond, col| cond.add(like(keyword, col)))
}
