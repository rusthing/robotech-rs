use sea_orm::DbErr;
use thiserror::Error;

/// # 数据库连接错误枚举
///
/// 该枚举定义了数据库连接建立与管理过程中可能出现的错误类型，
/// 包括获取/设置全局连接失败、配置错误与连接失败等。
#[derive(Error, Debug)]
pub enum DbError {
    #[error("Fail to get DB_CONN")]
    GetDbConn(),
    #[error("Fail to set DB_CONN")]
    SetDbConn(),
    #[error("Fail to app database: {0}")]
    Config(String),
    #[error("Fail to connect database: {0}")]
    Connect(DbErr),
}
