//! # 事务完整性测试
//!
//! 只验一件事：**框架的事务边界到底在哪**。用真实 PostgreSQL（事务语义没法 mock，
//! read committed 的可见性正是断言依据），覆盖五组场景：
//!
//! | 场景 | 断言点 |
//! |---|---|
//! | `#[db_unwrap(transaction_required)]` 正常结束 | 块内的写全部可见（提交生效） |
//! | 同上，块内第二句失败 | 第一句也不存在（回滚生效）——修复前事务空转，第一句会留下来 |
//! | 同上，调用方传 `Some(&tx)` | 调用方回滚后全部消失（方法不自作主张提交） |
//! | `#[svc] add(db = None)` | 钩子写的标记与业务行一起提交；`after_write` 时**独立连接**已可见 |
//! | `#[svc] add(db = None)` 钩子失败 | 业务行与钩子标记都不存在，且不触发 `after_write` |
//! | `#[svc] modify` 撞唯一键 | `before_modify` 用事务连接写的标记同样回滚 |
//!
//! ## 怎么跑
//!
//! 需要一个可写的 PostgreSQL，测试会在里面建独立 schema `robotech_tx_test`
//! （开始时 drop 重建，结束时删除）：
//!
//! ```text
//! ROBOTECH_TEST_DB_URL='postgres://msgrs:msgrs@127.0.0.1:5432/msgrs' \
//!   cargo test -p robotech-tests
//! ```
//!
//! 没设环境变量时测试**跳过**并打印提示，因此在没有数据库的 CI 上不会误报失败。

use idworker::IdWorkerConfig;
use robotech::api::U64;
use robotech::dao::{begin_transaction, commit_transaction};
use robotech::db::{DbConnConfig, get_db_conn, setup_db_conn};
use robotech::macros::db_unwrap;
use robotech::svc::SvcError;
use robotech_tests::dto::tx_item_dto::{TxItemAddDto, TxItemModifyDto};
use robotech_tests::probe;
use robotech_tests::svc::TxItemSvc;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};

/// 测试专用 schema（每次跑前 drop 重建）
const SCHEMA: &str = "robotech_tx_test";

/// 用调用方给的连接写两条标记
///
/// 这是 `#[db_unwrap(transaction_required)]` 的最小复现：块内 `?` 会直接跳出函数，
/// 事务应当随之回滚。
#[db_unwrap(transaction_required)]
async fn 写两条标记<C: ConnectionTrait>(
    db: Option<&C>,
    第一条: &str,
    第二条: &str,
) -> Result<(), SvcError> {
    probe::insert_marker(db, 第一条).await?;
    probe::insert_marker(db, 第二条).await?;
    Ok(())
}

/// 建 schema 与两张表，并初始化框架的全局连接池
async fn 准备环境(url: &str) -> DatabaseConnection {
    // ── 结构与业务表一致：id 无默认值、审计字段非空 ──
    let admin = Database::connect(url)
        .await
        .expect("连接测试数据库失败");
    admin
        .execute_unprepared(&format!("drop schema if exists {SCHEMA} cascade"))
        .await
        .expect("删除测试 schema 失败");
    admin
        .execute_unprepared(&format!("create schema {SCHEMA}"))
        .await
        .expect("创建测试 schema 失败");
    admin
        .execute_unprepared(&format!(
            r#"
            create table {SCHEMA}.tx_item (
               id         int8        not null,
               code       varchar(50) not null,
               name       varchar(50) null,
               creater_id int8        not null,
               create_ms  int8        not null,
               updater_id int8        not null,
               update_ms  int8        not null,
               constraint pk_tx_item primary key (id),
               constraint ak_code_tx_item unique (code)
            );
            create table {SCHEMA}.tx_marker (
               note varchar(200) not null,
               constraint pk_tx_marker primary key (note)
            );
            "#
        ))
        .await
        .expect("建表失败");

    // ── 框架连接池：走 schema-search-path，实体不用带 schema 前缀 ──
    let cfg: DbConnConfig = serde_json::from_str(&format!(
        r#"{{"url": "{url}", "max-connections": 5, "schema-search-path": "{SCHEMA}"}}"#
    ))
    .expect("构造数据库配置失败");
    setup_db_conn(cfg, &None).await.expect("初始化框架连接池失败");

    // ── 独立观测池：与写入方不同的物理连接，未提交的行在它眼里不可见 ──
    let mut opt = sea_orm::ConnectOptions::new(url);
    opt.set_schema_search_path(SCHEMA).max_connections(2);
    let probe_conn = Database::connect(opt).await.expect("连接观测池失败");
    probe::init(probe_conn);

    // DAO 插入时缺 id 会走 idworker，必须先初始化
    idworker::setup_id_worker(IdWorkerConfig::default(), &None).expect("初始化 idworker 失败");

    admin
}

fn 新增(code: &str) -> TxItemAddDto {
    let mut dto = TxItemAddDto::default();
    // id / 创建时间留空，交给 DAO 补（顺带覆盖这两条路径）
    dto.code = Some(code.to_string());
    // 业务列是 Option 时，AddDto 会再包一层：`Some(Some(v))` 才是「带上这个值」
    dto.name = Some(Some(format!("name-{code}")));
    dto._current_user_id = U64::from(1u64);
    dto
}

fn 修改(id: U64, code: &str) -> TxItemModifyDto {
    let mut dto = TxItemModifyDto::default();
    dto.id = Some(id);
    dto.code = Some(code.to_string());
    dto._current_user_id = U64::from(1u64);
    dto
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 事务完整性与钩子时序() {
    let Ok(url) = std::env::var("ROBOTECH_TEST_DB_URL") else {
        eprintln!(
            "跳过：未设置 ROBOTECH_TEST_DB_URL；\
             需要真实 PostgreSQL，例如 postgres://msgrs:msgrs@127.0.0.1:5432/msgrs"
        );
        return;
    };
    let admin = 准备环境(&url).await;

    // ─────────────────────────────────────────────
    // 1. db_unwrap(transaction_required)：正常结束要提交
    // ─────────────────────────────────────────────
    写两条标记(None::<&DatabaseConnection>, "commit-a", "commit-b")
        .await
        .expect("db_unwrap 正常路径不应失败");
    assert_eq!(
        probe::count_marker("commit-a").await.unwrap(),
        1,
        "提交后第一行必须可见"
    );
    assert_eq!(
        probe::count_marker("commit-b").await.unwrap(),
        1,
        "提交后第二行必须可见"
    );

    // ─────────────────────────────────────────────
    // 2. db_unwrap：块内第二句失败，第一句必须一起回滚
    //    （修复前事务开在 if 块里空转，第一句会直接落在池连接上留了下来）
    // ─────────────────────────────────────────────
    let err = 写两条标记(None::<&DatabaseConnection>, "rollback-x", "rollback-x").await;
    assert!(err.is_err(), "重复主键必须报错");
    assert_eq!(
        probe::count_marker("rollback-x").await.unwrap(),
        0,
        "第二句失败时第一句必须回滚，否则事务等于没开"
    );

    // ─────────────────────────────────────────────
    // 3. db_unwrap：调用方传连接即不提交，回滚权在调用方
    // ─────────────────────────────────────────────
    let conn = get_db_conn().expect("全局连接池已初始化");
    let tx = begin_transaction(conn.as_ref())
        .await
        .expect("开启调用方事务失败");
    写两条标记(Some(&tx), "caller-a", "caller-b")
        .await
        .expect("带连接调用不应失败");
    drop(tx); // 不提交 → 回滚
    assert_eq!(
        probe::count_marker("caller-a").await.unwrap(),
        0,
        "调用方回滚后，方法内部的写必须一起消失"
    );

    // ─────────────────────────────────────────────
    // 4. svc::add(db = None)：提交 + after_write 在提交之后
    // ─────────────────────────────────────────────
    probe::expect_code("ok-1");
    let ok = TxItemSvc::add(新增("ok-1"), None::<&DatabaseConnection>)
        .await
        .expect("新增应当成功");
    let ok_id = ok.extra.expect("新增应返回 Vo").id;
    assert!(
        probe::last_saw_committed(),
        "after_write 触发时独立连接必须已能看到这行；看不到说明通知发在提交之前"
    );
    assert_eq!(
        probe::count_item("ok-1").await.unwrap(),
        1,
        "提交后业务行必须可见"
    );
    assert_eq!(
        probe::count_marker("before_add:ok-1").await.unwrap(),
        1,
        "钩子里的标记与业务行同属一个事务，应一起提交"
    );

    // ─────────────────────────────────────────────
    // 5. svc::add：钩子失败 → 业务行与钩子标记都不留下，且不发通知
    // ─────────────────────────────────────────────
    probe::expect_code("boom");
    let boom = TxItemSvc::add(新增("boom"), None::<&DatabaseConnection>).await;
    assert!(boom.is_err(), "before_add 返回错误时必须整体失败");
    assert_eq!(
        probe::count_marker("before_add:boom").await.unwrap(),
        0,
        "钩子自己写的标记也要被回滚"
    );
    assert_eq!(
        probe::count_item("boom").await.unwrap(),
        0,
        "业务行不应留下"
    );
    assert!(
        !probe::last_saw_committed(),
        "失败时不应触发 after_write"
    );

    // ─────────────────────────────────────────────
    // 6. svc::add(db = Some(&tx))：写进调用方事务，回滚后一并消失
    // ─────────────────────────────────────────────
    probe::expect_code("caller-tx");
    let tx = begin_transaction(conn.as_ref())
        .await
        .expect("开启调用方事务失败");
    TxItemSvc::add(新增("caller-tx"), Some(&tx))
        .await
        .expect("带连接新增不应失败");
    assert!(
        !probe::last_saw_committed(),
        "调用方事务尚未提交，独立连接不该看到——带连接时通知只能先发"
    );
    drop(tx);
    assert_eq!(
        probe::count_item("caller-tx").await.unwrap(),
        0,
        "带连接时方法只写不提交，调用方回滚后必须消失"
    );
    assert_eq!(
        probe::count_marker("before_add:caller-tx").await.unwrap(),
        0,
        "钩子的标记同属调用方事务，同样回滚"
    );

    // ─────────────────────────────────────────────
    // 7. svc::modify 撞唯一键：before_modify 的标记一起回滚
    //    （证明钩子拿到的确实是事务连接，而不是池连接）
    // ─────────────────────────────────────────────
    TxItemSvc::add(新增("uniq-1"), None::<&DatabaseConnection>)
        .await
        .expect("预置 uniq-1 失败");
    let uniq2 = TxItemSvc::add(新增("uniq-2"), None::<&DatabaseConnection>)
        .await
        .expect("预置 uniq-2 失败")
        .extra
        .expect("新增应返回 Vo")
        .id;
    assert_eq!(
        probe::count_marker(&format!("before_modify:{}", uniq2.0)).await.unwrap(),
        0,
        "修改前的标记此时还不该存在"
    );

    let dup = TxItemSvc::modify(修改(uniq2, "uniq-1"), None::<&DatabaseConnection>).await;
    assert!(dup.is_err(), "改成已存在的编码必须报重复键");
    assert_eq!(
        probe::count_marker(&format!("before_modify:{}", uniq2.0)).await.unwrap(),
        0,
        "before_modify 用事务连接写的标记必须随业务写一起回滚"
    );
    assert_eq!(
        probe::count_item("uniq-2").await.unwrap(),
        1,
        "修改失败后原行必须保持原样"
    );
    // 说明：ok 场景的 id 由 idworker 生成，这里确认它与 Vo 一致
    assert!(ok_id.0 > 0, "id 应由 idworker 补齐，而不是 0");

    // ─────────────────────────────────────────────
    // 8. svc::add(db = Some(&tx)) + 调用方提交：写进调用方事务并随它落库
    // ─────────────────────────────────────────────
    probe::expect_code("caller-commit");
    let tx = begin_transaction(conn.as_ref())
        .await
        .expect("开启调用方事务失败");
    TxItemSvc::add(新增("caller-commit"), Some(&tx))
        .await
        .expect("带连接新增不应失败");
    commit_transaction(tx).await.expect("提交调用方事务失败");
    assert_eq!(
        probe::count_item("caller-commit").await.unwrap(),
        1,
        "调用方提交后，方法内写的数据必须落库"
    );

    // ─────────────────────────────────────────────
    // 收尾：删掉测试 schema
    // ─────────────────────────────────────────────
    admin
        .execute_unprepared(&format!("drop schema if exists {SCHEMA} cascade"))
        .await
        .expect("清理测试 schema 失败");
}
