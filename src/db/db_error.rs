use sea_orm::DbErr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Fail to config database: {0}")]
    Config(String),
    #[error("Fail to connect database: {0}")]
    Connect(DbErr),
    #[error("Fail to set DB_CONN")]
    SetDbConn(),
}
