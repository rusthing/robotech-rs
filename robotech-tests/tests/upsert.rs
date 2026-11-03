//! # upsert 语义：更新没命中就插入
//!
//! `save` 与 `upsert` 只差一步。`save` 传了 id 就直接 `modify`，记录不存在时把
//! `RecordNotUpdated` 原样抛给调用方；`upsert` 认出「没有这条记录」后退化成 `add`，
//! 而 `AddDto` 的 id 是「有则用、无则生成」，于是补建出来的是**请求里那个 id** 的行。
//!
//! 用真实 PostgreSQL 验五件事：
//!
//! | 场景 | 断言点 |
//! |---|---|
//! | 不传 id | 插入，id 由 idworker 生成 |
//! | 传 id 但记录不存在 | 插入且**沿用该 id**（不是换新 id） |
//! | 传已存在的 id | 更新，不新增行 |
//! | 更新撞唯一键 | 原样报重复键，**不**退化成插入（否则真实冲突被掩盖） |
//! | 钩子报 `NotFound` | 同样原样抛出，**不**退化成插入 |
//!
//! 后两组是这条逻辑的边界：`is_not_found` 只认 DAO 的 `RecordNotUpdated`，
//! 业务层 `NotFound` 不算——它的含义比「本行不存在」宽（钩子里「父记录查不到」
//! 用的就是它），拿它当判据会把该报的错变成一次脏写入。
//!
//! ## 怎么跑
//!
//! 需要一个可写的 PostgreSQL，测试会在里面建独立 schema `robotech_upsert_test`
//! （开始时 drop 重建，结束时删除）：
//!
//! ```text
//! ROBOTECH_TEST_DB_URL='postgres://msgrs:msgrs@127.0.0.1:5432/msgrs' \
//!   cargo test -p robotech-tests
//! ```
//!
//! 没设环境变量时测试**跳过**并打印提示。

use idworker::IdWorkerConfig;
use robotech::api::U64;
use robotech::dao::DaoError;
use robotech::db::{DbConnConfig, setup_db_conn};
use robotech::svc::SvcError;
use robotech_tests::dto::tx_item_dto::TxItemSaveDto;
use robotech_tests::probe;
use robotech_tests::svc::TxItemSvc;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};

/// 测试专用 schema（每次跑前 drop 重建）
const SCHEMA: &str = "robotech_upsert_test";

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

    // 不传 id 的那条路要 idworker 生成主键
    idworker::setup_id_worker(IdWorkerConfig::default(), &None).expect("初始化 idworker 失败");

    // 唯一键注册表：业务服务在 bootstrap 里初始化，这里手动补上。
    // 集成测试里 linkme 收集到的是空集合（lib 的目标文件没被引用，条目被链接器丢了），
    // 于是查不到具体键名，会走 `parse_duplicate_key` 的兜底分支——对第 4 组用例够用：
    // 我们要断言的是「错误类型是重复键」，不是「键名解析成了哪个列」。
    robotech::dao::init_unique_keys().expect("初始化唯一键注册表失败");
}

/// 构造 `upsert` 的入参（与 `save` 同一个 DTO）
fn 保存(id: Option<u64>, code: &str, name: Option<&str>) -> TxItemSaveDto {
    let mut dto = TxItemSaveDto::default();
    dto.id = id.map(U64::from);
    dto.code = Some(code.to_string());
    // 源字段是 Option<String> 时，DTO 里会再包一层：`Some(Some(v))` 才是「带上这个值」
    dto.name = Some(name.map(|v| v.to_string()));
    dto._current_user_id = U64::from(1u64);
    dto
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 更新没命中就按同一个id插入() {
    let Ok(url) = std::env::var("ROBOTECH_TEST_DB_URL") else {
        eprintln!(
            "跳过：未设置 ROBOTECH_TEST_DB_URL；\
             需要真实 PostgreSQL，例如 postgres://msgrs:msgrs@127.0.0.1:5432/msgrs"
        );
        return;
    };
    准备环境(&url).await;

    // ─────────────────────────────────────────────
    // 1. 不传 id：等同于 add，id 由 idworker 生成
    // ─────────────────────────────────────────────
    let created = TxItemSvc::upsert(保存(None, "upsert-a", None), None::<&DatabaseConnection>)
        .await
        .expect("不传 id 时应当直接插入");
    let generated_id = created.extra.expect("插入应返回 Vo").id;
    assert!(
        generated_id.value() > 0,
        "不传 id 时应当由 idworker 生成主键，实际 {generated_id}"
    );
    assert_eq!(
        probe::item_id("upsert-a").await.unwrap(),
        Some(generated_id.value() as i64),
        "落库的 id 必须就是返回的那个"
    );

    // ─────────────────────────────────────────────
    // 2. 传 id 但记录不存在：modify 报 RecordNotUpdated，退化成插入
    //    关键断言是「沿用请求里的 id」，不是重新生成一个
    // ─────────────────────────────────────────────
    const NEW_ID: u64 = 9_000_000_001;
    let inserted =
        TxItemSvc::upsert(保存(Some(NEW_ID), "upsert-b", Some("n1")), None::<&DatabaseConnection>)
            .await
            .expect("记录不存在时应退化成插入，而不是报错");
    assert_eq!(
        inserted.extra.expect("返回 Vo").id.value(),
        NEW_ID,
        "补建必须沿用请求里的 id"
    );
    assert_eq!(
        probe::item_id("upsert-b").await.unwrap(),
        Some(NEW_ID as i64),
        "库里那条记录的 id 必须是请求里的 id"
    );

    // ─────────────────────────────────────────────
    // 3. 传已存在的 id：正常更新，不新增行
    // ─────────────────────────────────────────────
    let updated =
        TxItemSvc::upsert(保存(Some(NEW_ID), "upsert-b", Some("n2")), None::<&DatabaseConnection>)
            .await
            .expect("记录已存在时应当更新");
    let updated = updated.extra.expect("返回 Vo");
    assert_eq!(updated.id.value(), NEW_ID, "更新不能换 id");
    assert_eq!(
        updated.name.as_deref(),
        Some("n2"),
        "更新后的值必须来自本次请求"
    );
    assert_eq!(
        probe::count_item("upsert-b").await.unwrap(),
        1,
        "更新不该多出一行"
    );

    // ─────────────────────────────────────────────
    // 4. 更新撞唯一键：原样报重复键，绝不能退化成插入
    //    换一个 id 做目标，避免 before_modify 钩子写的标记行（note 是主键）重复
    // ─────────────────────────────────────────────
    let other = TxItemSvc::add(
        robotech_tests::dto::tx_item_dto::TxItemAddDto {
            id: None,
            code: Some("upsert-c".to_string()),
            name: None,
            _current_ms: None,
            _current_user_id: U64::from(1u64),
        },
        None::<&DatabaseConnection>,
    )
    .await
    .expect("准备第 4 组的目标行失败");
    let other_id = other.extra.expect("返回 Vo").id;

    let err = TxItemSvc::upsert(
        保存(Some(other_id.value()), "upsert-b", None),
        None::<&DatabaseConnection>,
    )
    .await
    .expect_err("code 撞上 upsert-b 的 ak_code_tx_item，必须报重复键");

    assert!(
        matches!(err, SvcError::Dao(DaoError::DuplicateKey(_, _))),
        "错误应当是重复键（说明没有误判成「记录不存在」），实际：{err}"
    );
    assert_eq!(
        probe::count_item("upsert-c").await.unwrap(),
        1,
        "失败的 upsert 不该动目标行"
    );

    // ─────────────────────────────────────────────
    // 5. 钩子报 `NotFound`：同样原样抛出，绝不能退化成插入
    //    这是「只认 DAO 的 RecordNotUpdated」的钉子：业务层 NotFound 含义更宽
    //    （钩子里「父记录查不到」用的就是它），拿它当判据会把该报的错变成脏写入。
    // ─────────────────────────────────────────────
    let target = TxItemSvc::add(
        robotech_tests::dto::tx_item_dto::TxItemAddDto {
            id: None,
            code: Some("upsert-d".to_string()),
            name: None,
            _current_ms: None,
            _current_user_id: U64::from(1u64),
        },
        None::<&DatabaseConnection>,
    )
    .await
    .expect("准备第 5 组的目标行失败");
    let target_id = target.extra.expect("返回 Vo").id;

    let err = TxItemSvc::upsert(
        保存(
            Some(target_id.value()),
            "upsert-d",
            Some("trigger-not-found"),
        ),
        None::<&DatabaseConnection>,
    )
    .await
    .expect_err("before_modify 报 NotFound，必须原样失败而不是补建一行");

    assert!(
        matches!(err, SvcError::NotFound(_)),
        "钩子的 NotFound 应当原样抛出（不能当成「本行不存在」），实际：{err}"
    );
    assert_eq!(
        probe::count_item("upsert-d").await.unwrap(),
        1,
        "原行必须保持原样，且不能因为这次失败多出一行"
    );

    // 收尾：删掉测试 schema
    let admin = Database::connect(&url).await.expect("重连测试数据库失败");
    admin
        .execute_unprepared(&format!("drop schema if exists {SCHEMA} cascade"))
        .await
        .expect("删除测试 schema 失败");
}
