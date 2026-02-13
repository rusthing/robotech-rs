use sea_orm::DbErr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbConnError {
    #[error("Fail to get DB_CONN")]
    GetDbConn(),
    #[error("Fail to set DB_CONN")]
    SetDbConn(),
    #[error("Fail to app database: {0}")]
    Config(String),
    #[error("Fail to connect database: {0}")]
    Connect(DbErr),
}
