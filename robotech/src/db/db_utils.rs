use crate::db::{DbConnConfig, DbError};
use log::debug;
use robotech_macros::log_call;
use sea_orm::{ConnectOptions, Database, DbConn};
use std::sync::{Arc, RwLock};

/// 数据库连接
static DB_CONN: RwLock<Option<Arc<DbConn>>> = RwLock::new(None);

/// 获取App配置的只读访问
pub fn get_db_conn() -> Result<Arc<DbConn>, DbError> {
    let read_lock = DB_CONN.read().map_err(|_| DbError::GetDbConn())?;
    read_lock.clone().ok_or(DbError::GetDbConn())
}

/// 设置App配置
pub fn set_db_conn(value: DbConn) -> Result<(), DbError> {
    let mut write_lock = DB_CONN.write().map_err(|_| DbError::SetDbConn())?;
    *write_lock = Some(Arc::new(value));
    Ok(())
}

/// # 初始化数据库连接
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
#[log_call]
pub async fn init_db_conn(db_conn_config: DbConnConfig) -> Result<(), DbError> {
    debug!("init database...");

    if db_conn_config.url.is_empty() {
        Err(DbError::Config(
            "db.url (database connection string) item has not been configured yet".to_string(),
        ))?;
    }

    // 获取数据库配置
    let mut opt = ConnectOptions::new(db_conn_config.url);

    // 设置sql日志按什么级别输出
    opt.sqlx_logging_level(db_conn_config.log_level);

    // 连接数据库
    let connection = Database::connect(opt).await.map_err(DbError::Connect)?;
    // 设置数据库连接到全局变量中
    set_db_conn(connection)
}
