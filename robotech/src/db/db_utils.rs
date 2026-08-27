use crate::db::{DbConnConfig, DbError};
use arc_swap::ArcSwapOption;
use config::Value;
use sea_orm::{ConnectOptions, Database, DbConn};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

static KEY: &str = "db";
/// 数据库连接
static DB_CONN: ArcSwapOption<DbConn> = ArcSwapOption::const_empty();

/// 获取数据库连接的只读访问
pub fn get_db_conn() -> Result<Arc<DbConn>, DbError> {
    DB_CONN.load_full().ok_or(DbError::GetDbConn())
}

pub async fn setup_db_conn(
    db_conn_config: DbConnConfig,
    changed: &Option<HashMap<String, Value>>,
) -> Result<(), DbError> {
    info!("setup db connection...");
    if changed
        .as_ref()
        .map(|changed| changed.contains_key(KEY))
        .unwrap_or(true)
    {
        // 获取数据库配置
        let opt: ConnectOptions = db_conn_config.into();
        // 连接数据库
        let connection = Database::connect(opt).await.map_err(DbError::Connect)?;
        // 设置数据库连接到全局变量中
        DB_CONN.store(Some(Arc::new(connection)));
    }
    Ok(())
}
