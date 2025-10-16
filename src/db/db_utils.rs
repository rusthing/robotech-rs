use crate::db::DbSettings;
use log::info;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::sync::OnceLock;

/// 数据库连接
pub static DB_CONN: OnceLock<DatabaseConnection> = OnceLock::new();

/// 初始化数据库
pub async fn init_db(db_settings: DbSettings) {
    info!("init database...");

    // 获取数据库配置
    let mut opt = ConnectOptions::new(db_settings.url);

    // 设置sql日志按什么级别输出
    opt.sqlx_logging_level(log::LevelFilter::Trace);

    // 连接数据库
    let connection = Database::connect(opt)
        .await
        .expect("Failed to connect to the database");
    // 设置数据库连接到全局变量中
    DB_CONN
        .set(connection.clone())
        .expect("Unable to set database connector");
}
