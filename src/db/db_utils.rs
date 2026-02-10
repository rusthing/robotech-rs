use crate::db::db_error::DbError;
use crate::db::DbConfig;
use log::debug;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::sync::OnceLock;
use tracing::instrument;

/// 数据库连接
pub static DB_CONN: OnceLock<DatabaseConnection> = OnceLock::new();

/// # 初始化数据库
///
/// 该函数接收数据库配置信息，建立数据库连接，并将连接存储到全局静态变量 `DB_CONN` 中。
/// 连接建立后，可以通过 `DB_CONN` 全局访问数据库连接。
///
/// # 参数
///
/// * `db_config` - 数据库配置信息，包含连接数据库所需的信息
///
/// # Panics
///
/// * 如果数据库连接失败，程序将 panic
/// * 如果无法设置全局数据库连接，程序将 panic
#[instrument(level = "debug", err)]
pub async fn init_db(db_config: DbConfig) -> Result<(), DbError> {
    debug!("init database...");

    if db_config.url.is_empty() {
        Err(DbError::Config(
            "db.url (database connection string) item has not been configured yet".to_string(),
        ))?;
    }

    // 获取数据库配置
    let mut opt = ConnectOptions::new(db_config.url);

    // 设置sql日志按什么级别输出
    opt.sqlx_logging_level(log::LevelFilter::Trace);

    // 连接数据库
    let connection = Database::connect(opt).await.map_err(DbError::Connect)?;
    // 设置数据库连接到全局变量中
    DB_CONN
        .set(connection.clone())
        .map_err(|_| DbError::SetDbConn())?;
    Ok(())
}
