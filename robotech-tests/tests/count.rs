//! # count_by_query_dto 语义：按条件统计数量
//!
//! 验证 `#[dao]` / `#[svc]` 新生成的 `count_by_condition` / `count_by_query_dto`：
//!
//! | 场景 | 断言点 |
//! |---|---|
//! | 无条件 | 返回全表行数（U64，`value()` 与库里一致） |
//! | 等值条件 | 只数符合条件的行；`QueryDto` 的 `_page` / `_size` 不影响结果 |
//! | 关键字 | `_keyword` 走 LIKE_COLUMNS（code / name），与等值条件取 AND |
//! | 数据范围 | 注册策略后，count 与 list 叠加同一个范围条件 |
//!
//! 用真实 PostgreSQL 在独立 schema 建表跑，`extra` 取 `Ro<U64>`。
//!
//! ## 怎么跑
//!
//! ```text
//! ROBOTECH_TEST_DB_URL='postgres://msgrs:msgrs@127.0.0.1:5432/msgrs' \
//!   cargo test -p robotech-tests --test count
//! ```
//!
//! 没设环境变量时测试**跳过**并打印提示。

use idworker::IdWorkerConfig;
use robotech::api::U64;
use robotech::dao::{DataScope, set_data_scope};
use robotech::db::{DbConnConfig, setup_db_conn};
use robotech_tests::dto::tx_item_dto::{TxItemAddDto, TxItemQueryDto};
use robotech_tests::mo::tx_item::{Column, Entity};
use robotech_tests::svc::TxItemSvc;
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
    PaginatorTrait,
};

const SCHEMA: &str = "robotech_count_test";

/// 只放行 `ticket-` 前缀的编码；表名即实体模块名 `tx_item`
struct TicketScope;

impl DataScope for TicketScope {
    fn condition(&self, table: &str) -> Option<Condition> {
        if table == "tx_item" {
            Some(Condition::all().add(Column::Code.starts_with("ticket-")))
        } else {
            None
        }
    }
}

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

    let cfg: DbConnConfig = serde_json::from_str(&format!(
        r#"{{"url": "{url}", "max-connections": 5, "schema-search-path": "{SCHEMA}"}}"#
    ))
    .expect("构造数据库配置失败");
    setup_db_conn(cfg, &None)
        .await
        .expect("初始化框架连接池失败");
    idworker::setup_id_worker(IdWorkerConfig::default(), &None).expect("初始化 idworker 失败");
}

/// 插入一行，审计字段沿用框架兜底
async fn 建行(code: &str, name: Option<&str>) {
    let mut dto = TxItemAddDto::default();
    dto.code = Some(code.to_string());
    dto.name = Some(name.map(|v| v.to_string()));
    dto._current_user_id = U64::from(1u64);
    TxItemSvc::add(dto, None::<&DatabaseConnection>)
        .await
        .unwrap_or_else(|e| panic!("建行 {code} 失败：{e}"));
}

/// 用直连连接数全表，作为 count 结果的人工对照
async fn 库里总数(url: &str) -> u64 {
    let mut opt = sea_orm::ConnectOptions::new(url);
    opt.set_schema_search_path(SCHEMA);
    let conn = Database::connect(opt).await.expect("连接测试 schema 失败");
    Entity::find().count(&conn).await.expect("数全表失败")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 按条件统计数量() {
    let Ok(url) = std::env::var("ROBOTECH_TEST_DB_URL") else {
        eprintln!(
            "跳过：未设置 ROBOTECH_TEST_DB_URL；\
             需要真实 PostgreSQL，例如 postgres://msgrs:msgrs@127.0.0.1:5432/msgrs"
        );
        return;
    };
    准备环境(&url).await;

    建行("ticket-001", Some("alpha")).await;
    建行("ticket-002", Some("beta")).await;
    建行("other-001", Some("gamma")).await;

    // 1. 无条件：等于全表行数
    let ro = TxItemSvc::count_by_query_dto(TxItemQueryDto::default(), None::<&DatabaseConnection>)
        .await
        .expect("无条件统计失败");
    assert_eq!(
        ro.extra.map(|v| v.value()),
        Some(库里总数(&url).await),
        "无条件 count 应等于全表行数"
    );

    // 2. 等值条件 + 分页字段被忽略：只数 code = ticket-001
    let mut dto = TxItemQueryDto::default();
    dto.code = Some("ticket-001".to_string());
    dto._page = Some(U64::from(99));
    dto._size = Some(U64::from(1));
    let ro = TxItemSvc::count_by_query_dto(dto, None::<&DatabaseConnection>)
        .await
        .expect("等值统计失败");
    assert_eq!(ro.extra.map(|v| v.value()), Some(1), "等值条件应只数一行");

    // 3. 关键字：_keyword 走 code / name 两列的 LIKE，与等值条件取 AND
    let mut dto = TxItemQueryDto::default();
    dto._keyword = Some("beta".to_string());
    let ro = TxItemSvc::count_by_query_dto(dto, None::<&DatabaseConnection>)
        .await
        .expect("关键字统计失败");
    assert_eq!(
        ro.extra.map(|v| v.value()),
        Some(1),
        "name 含 beta 的只有一行"
    );

    // 4. 数据范围：注册后 count 同样叠加范围条件
    // Arc<dyn DataScope> 没有 Debug，不能用 expect
    if let Err(existing) = set_data_scope(TicketScope) {
        panic!(
            "数据范围只能注册一次，已有策略：{}",
            std::any::type_name_of_val(&*existing)
        );
    }
    let ro = TxItemSvc::count_by_query_dto(TxItemQueryDto::default(), None::<&DatabaseConnection>)
        .await
        .expect("带数据范围统计失败");
    assert_eq!(
        ro.extra.map(|v| v.value()),
        Some(2),
        "数据范围应把 other-001 排除"
    );

    let mut dto = TxItemQueryDto::default();
    dto._keyword = Some("gamma".to_string());
    let ro = TxItemSvc::count_by_query_dto(dto, None::<&DatabaseConnection>)
        .await
        .expect("带数据范围关键字统计失败");
    assert_eq!(
        ro.extra.map(|v| v.value()),
        Some(0),
        "被数据范围排除的行不应被关键字命中"
    );
}
