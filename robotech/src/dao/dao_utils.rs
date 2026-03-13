use crate::dao::eo::ForeignKey;
use crate::dao::DaoError;
use crate::db::get_db_conn;
use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbConn, TransactionTrait};
use std::collections::HashMap;
use std::sync::Arc;

pub fn push_unique_field(
    unique_fields: &mut HashMap<String, String>,
    table: String,
    fields: String,
    name: String,
) {
    let fields: Vec<String> = fields.split(',').map(|s| s.trim().to_string()).collect();
    if fields.len() == 0 {
        panic!("No fields provided for unique index")
    } else if fields.len() == 1 {
        // 添加postgre类的key
        let field = fields.get(0).unwrap(); // 长度为1，肯定存在
        unique_fields.insert(field.clone(), name.clone());
        // 添加mysql9类的key
        let key = format!("{table}.ak_{field}");
        unique_fields.insert(key, name);
    } else {
        // 添加postgre类的key
        let key = fields.join(", ");
        unique_fields.insert(key, name.clone());
        // 添加mysql9类的key
        let key = format!("{table}.ak_{}", fields.join("_and_"));
        unique_fields.insert(key, name);
    }
}

pub fn calc_key_of_foreign_key(fk_table: &str, fk_column: &str, pk_table: &str) -> String {
    format!("{fk_table}_{fk_column}_{pk_table}")
}

pub fn push_feign_key(
    feign_keys: &mut HashMap<String, ForeignKey>,
    fk_table: String,
    fk_table_comment: String,
    fk_column: String,
    pk_table: String,
    pk_table_comment: String,
) {
    feign_keys.insert(
        calc_key_of_foreign_key(&fk_table, &fk_column, &pk_table),
        ForeignKey::builder()
            .fk_table_comment(fk_table_comment)
            .fk_table(fk_table)
            .fk_column(fk_column)
            .pk_table_comment(pk_table_comment)
            .pk_table(pk_table)
            .build(),
    );
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
