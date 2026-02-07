use crate::dao::DaoError;
use crate::db::DB_CONN;
use sea_orm::{DatabaseConnection, DatabaseTransaction, TransactionTrait};

pub fn unwrap_db(db: Option<&DatabaseConnection>) -> Result<&DatabaseConnection, DaoError> {
    if let Some(db) = db {
        Ok(db)
    } else {
        if let Some(db_conn) = DB_CONN.get() {
            Ok(db_conn)
        } else {
            Err(DaoError::GetDbConn())?
        }
    }
}

pub async fn begin_transaction(db: &DatabaseConnection) -> Result<DatabaseTransaction, DaoError> {
    Ok(db.begin().await?)
}

pub async fn commit_transaction(tx: DatabaseTransaction) -> Result<(), DaoError> {
    Ok(tx.commit().await?)
}
