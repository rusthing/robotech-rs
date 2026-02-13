use crate::db_conn::DbConfig;
use crate::db_conn::db_conn_error::DbConnError;
use log::debug;
use sea_orm::{ConnectOptions, Database, DbConn};
use std::sync::{Arc, RwLock};
use tracing::instrument;

/// 数据库连接
static DB_CONN: RwLock<Option<Arc<DbConn>>> = RwLock::new(None);

/// 获取App配置的只读访问
pub fn get_db_conn() -> Result<Arc<DbConn>, DbConnError> {
    let read_lock = DB_CONN.read().map_err(|_| DbConnError::GetDbConn())?;
    read_lock.clone().ok_or(DbConnError::GetDbConn())
}

/// 设置App配置
pub fn set_db_conn(value: DbConn) -> Result<(), DbConnError> {
    let mut write_lock = DB_CONN.write().map_err(|_| DbConnError::SetDbConn())?;
    *write_lock = Some(Arc::new(value));
    Ok(())
}

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
pub async fn init_db(db_config: DbConfig) -> Result<(), DbConnError> {
    debug!("init database...");

    if db_config.url.is_empty() {
        Err(DbConnError::Config(
            "db_conn.url (database connection string) item has not been configured yet".to_string(),
        ))?;
    }

    // 获取数据库配置
    let mut opt = ConnectOptions::new(db_config.url);

    // 设置sql日志按什么级别输出
    opt.sqlx_logging_level(log::LevelFilter::Trace);

    // 连接数据库
    let connection = Database::connect(opt).await.map_err(DbConnError::Connect)?;
    // 设置数据库连接到全局变量中
    set_db_conn(connection)
}
