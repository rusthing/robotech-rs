//! # sync 语义：带版本门槛的条件更新
//!
//! `sync` 是给多源同步用的：按 id 更新，但只在「存储的 `update_ms` < 传入
//! `update_ms`」时才真正改写；否则影响 0 行、**不报错**。用途是中心库只接受
//! 比自己当前版本更新的写入——迟到、乱序、重复的重放被静默丢弃，不会把
//! 新数据覆盖成旧数据。
//!
//! 用真实 PostgreSQL 验五件事（单函数串行执行，避免并行建 schema 竞态）：
//!
//! | 场景 | 断言点 |
//! |---|---|
//! | 传入版本更新 | 影响行数 1，字段与版本号都刷新 |
//! | 传入版本更旧 | 影响行数 0（不报错），字段与版本号原样保留 |
//! | 版本完全相同（重放） | 影响行数 0，幂等 |
//! | 缺 id | 校验失败，不碰数据 |
//! | 缺版本号 | 校验失败，不碰数据 |
//!
//! 第 2、3 组是这条逻辑的核心：0 行是**正常结果**而不是错误，调用方靠
//! `Ro.extra` 里的行数区分「已应用 / 被拦下」。
//!
//! ## 怎么跑
//!
//! 需要一个可写的 PostgreSQL，测试会在里面建独立 schema `robotech_sync_test`
//! （开始时 drop 重建，结束时删除）：
//!
//! ```text
//! ROBOTECH_TEST_DB_URL='postgres://msgrs:msgrs@127.0.0.1:5432/msgrs' \
//!   cargo test -p robotech-tests --test sync
//! ```
//!
//! 没设环境变量时测试**跳过**并打印提示。

use idworker::IdWorkerConfig;
use robotech::api::U64;
use robotech::db::{DbConnConfig, setup_db_conn};
use robotech::svc::SvcError;
use robotech_tests::dto::tx_item_dto::{TxItemAddDto, TxItemSyncDto};
use robotech_tests::probe;
use robotech_tests::svc::TxItemSvc;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};

/// 测试专用 schema（每次跑前 drop 重建）
const SCHEMA: &str = "robotech_sync_test";

/// 建 schema 与两张表，并初始化框架连接池 / 观测池 / idworker
async fn 准备环境(url: &str) {
    let admin = Database::connect(url).await.expect("连接测试数据库失败");
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
    setup_db_conn(cfg, &None)
        .await
        .expect("初始化框架连接池失败");

    // ── 观测池：与写入方不同的物理连接，只看得到已提交的数据 ──
    let mut opt = sea_orm::ConnectOptions::new(url);
    opt.set_schema_search_path(SCHEMA).max_connections(2);
    let probe_conn = Database::connect(opt).await.expect("连接观测池失败");
    probe::init(probe_conn);

    idworker::setup_id_worker(IdWorkerConfig::default(), &None).expect("初始化 idworker 失败");
}

/// 构造 `sync` 的入参
fn 同步(id: u64, code: &str, name: Option<&str>, update_ms: u64) -> TxItemSyncDto {
    let mut dto = TxItemSyncDto::default();
    dto.id = Some(U64::from(id));
    dto.code = Some(code.to_string());
    // 源字段是 Option<String> 时，DTO 里会再包一层：`Some(Some(v))` 才是「带上这个值」
    dto.name = Some(name.map(|v| v.to_string()));
    dto.update_ms = Some(U64::from(update_ms));
    dto._current_user_id = U64::from(1u64);
    dto
}

/// 建一行初始数据，返回 (id, 库里的 update_ms)
async fn 建初始行(code: &str) -> (u64, i64) {
    let ro = TxItemSvc::add(
        TxItemAddDto {
            id: None,
            code: Some(code.to_string()),
            name: None,
            _current_ms: None,
            _current_user_id: U64::from(1u64),
        },
        None::<&DatabaseConnection>,
    )
    .await
    .expect("建初始行失败");
    let id = ro.extra.expect("add 应返回 Vo").id.value();
    let (version, _) = probe::item_version_and_name(code)
        .await
        .expect("读初始版本失败")
        .expect("初始行应存在");
    (id, version)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 只接受比自己新的版本() {
    let Ok(url) = std::env::var("ROBOTECH_TEST_DB_URL") else {
        eprintln!(
            "跳过：未设置 ROBOTECH_TEST_DB_URL；\
             需要真实 PostgreSQL，例如 postgres://msgrs:msgrs@127.0.0.1:5432/msgrs"
        );
        return;
    };
    准备环境(&url).await;

    // ─────────────────────────────────────────────
    // 1. 传入版本更新：应用，字段与版本号都刷新
    // ─────────────────────────────────────────────
    let (id, stored_ms) = 建初始行("sync-newer").await;
    let newer = stored_ms as u64 + 100_000;

    let ro = TxItemSvc::sync(
        同步(id, "sync-newer", Some("v2"), newer),
        None::<&DatabaseConnection>,
    )
    .await
    .expect("版本更新应成功");
    assert_eq!(ro.extra, Some(1), "应影响 1 行");

    let (ms, name) = probe::item_version_and_name("sync-newer")
        .await
        .expect("读取失败")
        .expect("行应存在");
    assert_eq!(ms as u64, newer, "版本号应刷新为传入值");
    assert_eq!(name.as_deref(), Some("v2"), "字段应被改写");

    // ─────────────────────────────────────────────
    // 2. 传入版本更旧：影响 0 行且不报错——这正是 sync 存在的意义，
    //    旧包不能把新数据覆盖回去
    // ─────────────────────────────────────────────
    let older = (stored_ms as u64).saturating_sub(100_000);
    let ro = TxItemSvc::sync(
        同步(id, "sync-newer", Some("旧包"), older),
        None::<&DatabaseConnection>,
    )
    .await
    .expect("0 行不该报错");
    assert_eq!(ro.extra, Some(0), "旧包应被版本门槛拦下");

    let (ms, name) = probe::item_version_and_name("sync-newer")
        .await
        .expect("读取失败")
        .expect("行应存在");
    assert_eq!(ms as u64, newer, "版本号应保持新值");
    assert_eq!(name.as_deref(), Some("v2"), "字段不应被旧包覆盖");

    // ─────────────────────────────────────────────
    // 3. 版本完全相同（同一份数据重放）：同样拦下，幂等
    // ─────────────────────────────────────────────
    let ro = TxItemSvc::sync(
        同步(id, "sync-newer", Some("重放"), newer),
        None::<&DatabaseConnection>,
    )
    .await
    .expect("重放不该报错");
    assert_eq!(ro.extra, Some(0), "相同版本应被拦下（WHERE 是严格小于）");

    let (ms, name) = probe::item_version_and_name("sync-newer")
        .await
        .expect("读取失败")
        .expect("行应存在");
    assert_eq!(ms as u64, newer, "重放不应推进版本号");
    assert_eq!(name.as_deref(), Some("v2"), "重放不应改写字段");

    // ─────────────────────────────────────────────
    // 4/5. 缺 id、缺版本号：DTO 校验失败，且不碰数据
    //      （update_ms 必填是刻意设计：sync 本就是带版本的条件写，没版本没意义）
    // ─────────────────────────────────────────────
    let (id2, stored_ms2) = 建初始行("sync-missing").await;

    let mut no_id = 同步(id2, "sync-missing", None, stored_ms2 as u64 + 1);
    no_id.id = None;
    let err = TxItemSvc::sync(no_id, None::<&DatabaseConnection>)
        .await
        .expect_err("缺 id 应校验失败");
    assert!(
        matches!(err, SvcError::Validations(_) | SvcError::Validation(_)),
        "应报参数校验错误，实际 {err:?}"
    );

    let mut no_ms = 同步(id2, "sync-missing", None, stored_ms2 as u64 + 1);
    no_ms.update_ms = None;
    let err = TxItemSvc::sync(no_ms, None::<&DatabaseConnection>)
        .await
        .expect_err("缺版本号应校验失败");
    assert!(
        matches!(err, SvcError::Validations(_) | SvcError::Validation(_)),
        "应报参数校验错误，实际 {err:?}"
    );

    let (ms, name) = probe::item_version_and_name("sync-missing")
        .await
        .expect("读取失败")
        .expect("行应存在");
    assert_eq!(ms, stored_ms2, "失败不应推进版本号");
    assert_eq!(name, None, "失败不应改写字段");

    // 收尾：删掉测试 schema
    let admin = Database::connect(&url).await.expect("重连测试数据库失败");
    admin
        .execute_unprepared(&format!("drop schema if exists {SCHEMA} cascade"))
        .await
        .expect("删除测试 schema 失败");
}
