use crate::dao::DaoError;
use crate::db::get_db_conn;
use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbConn, TransactionTrait};
use std::collections::HashMap;
use std::sync::Arc;

pub fn push_unique_field(
    unique_fields: &mut HashMap<&'static str, &'static str>,
    table: &'static str,
    fields: &'static str,
    name: &'static str,
) {
    let fields: Vec<&str> = fields.split(',').map(|s| s.trim()).collect();
    if fields.len() == 0 {
        panic!("No fields provided for unique index")
    } else if fields.len() == 1 {
        // 添加postgre类的key
        let field = fields.get(0).unwrap(); // 长度为1，肯定存在
        unique_fields.insert(field, name);
        // 添加mysql9类的key
        let key = Box::leak(format!("{table}.ak_{field}").into_boxed_str());
        unique_fields.insert(key, name);
    } else {
        // 添加postgre类的key
        let key = Box::leak(fields.join(", ").into_boxed_str());
        unique_fields.insert(key, name);
        // 添加mysql9类的key
        let key = Box::leak(format!("{table}.ak_{}", fields.join("_and_")).into_boxed_str());
        unique_fields.insert(key, name);
    }
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
