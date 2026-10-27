//! # 观测用的独立连接池
//!
//! 事务测试的关键不是「写完之后查得到」，而是**在哪个时刻**、**从哪条连接**查得到：
//!
//! - 独立连接池 = 与写入方不同的物理连接。PostgreSQL 的 read committed 下，未提交的
//!   行对其它连接不可见，所以「从独立连接查得到」等价于「已经提交」。
//! - `after_write` 没有 `db` 参数，只能拿到全局状态，因此这里用一个 `OnceLock` 存放
//!   独立连接池，钩子里通过它反查可见性；测试再断言钩子记下的结论。
//!
//! 唯一的坑：绝不可以用框架的全局连接池（`get_db_conn()`）来查——池子可能把刚提交时
//! 用过的那条连接再发出来，那条连接自己能看到本会话的未提交数据，测试就成了假阳性。

use robotech::svc::SvcError;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// 独立连接池（测试启动时注入）
static PROBE: OnceLock<DatabaseConnection> = OnceLock::new();

/// 注入独立连接池
pub fn init(conn: DatabaseConnection) {
    PROBE.set(conn).expect("probe 连接池只能初始化一次");
}

fn probe() -> &'static DatabaseConnection {
    PROBE.get().expect("probe 连接池尚未初始化")
}

/// `after_write` 要检查的编码（由测试在发起写操作前设置）
static EXPECTED_CODE: Mutex<Option<String>> = Mutex::new(None);

/// `after_write` 在**独立连接**上是否已经看到了这行
static LAST_SAW_COMMITTED: AtomicBool = AtomicBool::new(false);


/// 设置本轮 `after_write` 的观测目标，并清掉上一轮的结论
pub fn expect_code(code: &str) {
    *EXPECTED_CODE.lock().expect("锁被污染") = Some(code.to_string());
    LAST_SAW_COMMITTED.store(false, Ordering::SeqCst);
}

/// 取上一轮 `after_write` 的结论
pub fn last_saw_committed() -> bool {
    LAST_SAW_COMMITTED.load(Ordering::SeqCst)
}

/// 供 `after_write` 钩子调用：从独立连接查目标编码是否可见
///
/// 没有设置目标时记 `false`——这会让「忘记设置目标」的测试直接失败，而不是静默通过。
pub async fn observe_after_write() {
    let code = EXPECTED_CODE.lock().expect("锁被污染").clone();
    let Some(code) = code else {
        LAST_SAW_COMMITTED.store(false, Ordering::SeqCst);
        return;
    };
    let seen = count_item(&code).await.unwrap_or_default() > 0;
    LAST_SAW_COMMITTED.store(seen, Ordering::SeqCst);
}

/// 从独立连接统计 `tx_item` 中某编码的行数
pub async fn count_item(code: &str) -> Result<i64, SvcError> {
    count(&format!(
        "select count(*) from tx_item where code = '{}'",
        sql_escape(code)
    ))
    .await
}

/// 从独立连接统计 `tx_marker` 中某备注的行数
pub async fn count_marker(note: &str) -> Result<i64, SvcError> {
    count(&format!(
        "select count(*) from tx_marker where note = '{}'",
        sql_escape(note)
    ))
    .await
}

/// 从独立连接取某编码的 ID
pub async fn item_id(code: &str) -> Result<Option<i64>, SvcError> {
    let stmt = sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        format!(
            "select id from tx_item where code = '{}'",
            sql_escape(code)
        ),
    );
    let row = probe()
        .query_one_raw(stmt)
        .await
        .map_err(|e| SvcError::Runtime(anyhow::Error::new(e)))?;
    Ok(row.map(|r| r.try_get::<i64>("", "id").unwrap_or_default()))
}

async fn count(sql: &str) -> Result<i64, SvcError> {
    let stmt = sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Postgres, sql.to_string());
    let row = probe()
        .query_one_raw(stmt)
        .await
        .map_err(|e| SvcError::Runtime(anyhow::Error::new(e)))?;
    Ok(row
        .map(|r| r.try_get::<i64>("", "count").unwrap_or_default())
        .unwrap_or_default())
}

/// 用**调用方给的连接**插一条标记行
///
/// 钩子拿到的 `db` 是不是事务连接，靠这张表验证：钩子里写的标记若随着业务写一起
/// 回滚，说明两者在同一个事务里。
pub async fn insert_marker<C: ConnectionTrait>(db: &C, note: &str) -> Result<(), SvcError> {
    db.execute_unprepared(&format!(
        "insert into tx_marker (note) values ('{}')",
        sql_escape(note)
    ))
    .await
    .map_err(|e| SvcError::Runtime(anyhow::Error::new(e)))?;
    Ok(())
}

/// 单引号转义（测试里只拼自己的常量，不接外部输入）
fn sql_escape(v: &str) -> String {
    v.replace('\'', "''")
}
