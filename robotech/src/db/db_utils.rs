use crate::db::db_conn_config::DB_CONN_CONFIG_KEY;
use crate::db::{DbConnConfig, DbError};
use arc_swap::ArcSwapOption;
use config::Value;
use sea_orm::{ConnectOptions, Database, DbConn};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;
use wheel_rs::config_utils::has_config_changed;

/// 数据库连接
static DB_CONN: ArcSwapOption<DbConn> = ArcSwapOption::const_empty();

/// # 获取数据库连接
///
/// 返回全局数据库连接池的只读访问引用。
///
/// ## 返回值
/// 返回数据库连接引用
///
/// ## 错误
/// 数据库连接尚未初始化时返回 `DbError::GetDbConn`
pub fn get_db_conn() -> Result<Arc<DbConn>, DbError> {
    DB_CONN.load_full().ok_or(DbError::GetDbConn())
}

/// # 初始化数据库连接
///
/// 根据连接配置建立数据库连接并存入全局连接池。
/// 若传入的配置变更信息显示数据库配置未变化，则跳过重建连接。
///
/// ## 参数
/// * `db_conn_config` - 数据库连接配置
/// * `changed` - 配置变更信息，用于判断数据库配置是否发生变化
///
/// ## 返回值
/// 初始化成功返回 `Ok(())`
///
/// ## 错误
/// 数据库连接建立失败时返回 `DbError::Connect`
pub async fn setup_db_conn(
    db_conn_config: DbConnConfig,
    changed: &Option<HashMap<String, Value>>,
) -> Result<(), DbError> {
    info!("setup db connection...: {db_conn_config:?}");
    if changed
        .as_ref()
        .map(|changed| has_config_changed(DB_CONN_CONFIG_KEY, changed))
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
