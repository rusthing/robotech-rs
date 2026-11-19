# AGENTS.md — robotech-rs 框架规范

本文件是本仓库的**唯一规范入口**，写给两类读者：在本仓库里改代码的人，以及 AI 编码代理。
`README.md` / `README_zh.md` 面向**使用者**（怎么接入框架）；本文件面向**改造者**（怎么改框架、
新增能力时该守什么约束）。

优先级：本文件 > `README*.md` > 既有代码写法。三者冲突时以本文件为准；本文件未覆盖的，
先找仓库里已有的同类实现照抄，不要自创风格。

---

## 1. 仓库定位

RoboTech 平台的**框架层**，不是业务工程。核心原则一句话：

> **机制在框架，语义在业务。**

框架不替业务决定「当前用户是谁、能看到哪些数据、模板变量从哪来、业务码文案是什么」，
只提供**可注入的挂点**（trait、注册函数、钩子、distributed slice）。判断一段代码该不该
进本仓库，就用这条：如果它依赖具体的表名、字段名、业务枚举或某家公司的业务规则，它不属于这里。

## 2. crate 边界

| crate | 职责 | 发布 | 硬边界 |
|---|---|---|---|
| `robotech/` | 运行时框架（配置、日志、DB、Web、微服务、MQ、缓存、模板调用、RO） | 是，MIT | 按 feature 裁剪；`app` / `macros` 不得间接引入 axum / sea-orm / utoipa |
| `robotech-macros/` | 过程宏（`dao` / `svc` / `ctrl` / `router` / `dic` / `vo` / `crud_dto` …） | 是 | **只能依赖** `syn` / `quote` / `proc-macro2` / `wheel-rs`；**不得**依赖 `robotech`（循环）；不得在宏里做 I/O、读文件、读环境变量 |
| `robotech-template/` | 模板渲染内核（多引擎） | 是 | **只提供机制**：不读配置、不写日志、不访问数据库；时区与「当前时间」由调用方塞进 `FilterContext`，变量优先级由调用方用 `PriorityVariables` 组装 |
| `robotech-tests/` | 集成测试（事务、钩子、排序、刷新、upsert 语义） | 否（`publish = false`） | 只在真实 PostgreSQL/NATS 上验证；表结构在临时 schema 自建，**不读业务库** |
| `tools/model_comment/sync.py` | DTO 注释标签 ↔ PowerDesigner 模型 / 建表 SQL 同步 | — | 必须幂等：已有 `@len` / `@range` / `@enum` 行整行替换，散文原样保留 |

改动落点速查：

| 要做什么 | 落在哪 |
|---|---|
| 新过程宏 | `robotech-macros/src/<name>/mod.rs` 实现 + `robotech-macros/src/lib.rs` 导出（`robotech/src/macros` 已 `pub use robotech_macros::*` 自动透出） |
| 新对外包装类型（`U64`、`F64`、`Duration`、`MaskedMobile` …） | `robotech/src/api/`，并在 `openapi` feature 下补 `ToSchema` |
| 数据访问能力（事务、检索、树编码、排序、数据范围） | `robotech/src/dao/` |
| Web 中间件 / 中间件扩展点 | `robotech/src/web/middleware/`；全局业务中间件走 `ROUTER_LAYER_SLICE` |
| 服务治理（配置中心 / 注册中心 / Feign） | `robotech/src/micro_svc/`，契约层与实现层分开 |
| 模板语义 | 不进 `robotech-template`（那是纯机制），进调用方业务 crate |

## 3. 工具链、依赖与构建

- **edition 2024**（要求 `rustc >= 1.85`；本机实测 `rustc 1.98.1`）。
- **依赖版本只写在根 `Cargo.toml` 的 `[workspace.dependencies]`**，各 crate 内一律
  `dep = { workspace = true }`，禁止在子 crate 里写内联版本号。
- **新增依赖的固定顺序**：先加 `[workspace.dependencies]` → 子 crate 声明 `optional = true` →
  再挂到对应 feature。跳过任何一步都会让 feature 裁剪失效。
- `[patch.crates-io]` 里 `wheel-rs`、`idworker` 走本地路径，`robotech-macros` 指向本仓库。
  **不要为了让某台机器编译过去而删改 patch**；`idworker` 那条注释已写明「1.5.0 发布后可删」，
  只在真正发布后清理。
- 本机 PATH 里可能没有 cargo（macOS 默认装到 `~/.cargo/bin`），命令用绝对路径：
  `~/.cargo/bin/cargo`。

常用命令：

```bash
~/.cargo/bin/cargo check -p robotech --features app,macros
~/.cargo/bin/cargo check -p robotech --features app,macros,web,db
~/.cargo/bin/cargo check -p robotech --features app,macros,api-client
~/.cargo/bin/cargo fmt --all --check
~/.cargo/bin/cargo clippy -p robotech --all-targets --features app,macros,web,db,refresh
~/.cargo/bin/cargo test -p robotech
~/.cargo/bin/cargo test -p robotech-tests      # 见 §10，需要真实 PG
```

> **格式化只碰自己改过的文件**：`cargo fmt --all --check` 在当前 HEAD 上有 200+ 处历史差异
> （`robotech-macros`、`robotech-template`、`robotech-tests` 里都有）。直接跑 `cargo fmt --all`
> 会把无关文件的改写一起塞进提交、污染 diff。改完用
> `~/.cargo/bin/rustfmt --edition 2024 <改动的文件>` 单文件格式化，其余差异**保持原样**。
> **不要发起「全仓 fmt 对齐」的 style 提交** —— 历史差异是已知状态，不做统一格式化。

## 4. feature 规范

当前矩阵（以 `robotech/Cargo.toml` 为准）：

| feature | 组合 | 说明 |
|---|---|---|
| `app` | — | 配置、日志、环境、信号、`SvcError`、`bootstrap!`；含 `idworker/utils` |
| `macros` | — | 过程宏 |
| `openapi` | — | `U8..U128` / `Duration` / `Ro` / `PageRx` 的 `ToSchema`；`#[vo]` / `#[crud_dto]` / `#[dic]` 依赖它 |
| `orm` | — | `U64` / `Duration` ↔ sea-orm `Value` 互转 + 检索参数翻译 |
| `db` | `app, orm, sqlx, regex, once_cell, linkme` | 数据访问 |
| `web` | `app, openapi, axum, tower, rustls, socket2 …` | Web 服务 |
| `micro-svc-client` | — | 治理客户端契约 + Consul/Etcd/Nacos 实现，**不含 axum** |
| `config-center` | `micro-svc-client, toml` | 配置中心 |
| `registry-center` | `micro-svc-client, web` | 注册中心（实例注册需要 web 层端口/健康检查 URL） |
| `feign` | `api-client, registry-center` | 声明式服务间调用 |
| `api-client` | `openapi, reqwest, http, jsonwebtoken` | HTTP 客户端；**不带 orm/sqlx** |
| `mqtt` / `nats` / `rabbitmq` / `redis` / `influxdb` | — | MQ、缓存、时序库 |
| `refresh` | `nats` | JetStream 变更广播 + 订阅自愈 |
| `local-cache` / `distributed-cache` | `refresh` / `redis` | 缓存实现 |

四条硬规则：

1. **正交**：任何新能力都要能在「只开 `app,macros`」的纯后台进程里编过去。
2. **单向**：底层不得反向依赖上层（`micro-svc-client` 不许引 `web`，`orm` 不许引 `db`）。
3. **门控写法统一**：`lib.rs` 里 `#[cfg(feature = "...")] pub mod x;`；模块内子文件用
   `mod x;` + `#[cfg(feature = "...")] pub use x::*;` 三行式（见 §6）。
4. **公开能力变更要同步三处**：`robotech/src/lib.rs` 的模块表与 feature 矩阵、`README.md`、
   `README_zh.md`。中英文档不允许只改一边。

## 5. 分层结构、命名与路由推导

业务 crate（如 `adm-svr`、`robotech-tests`）一律按下面的分层组织，**层名固定**：

| 层 | 目录 | 命名 | 生成方式 |
|---|---|---|---|
| mo（Model Object） | `src/mo/*.rs` | `TxItem`（表名转大驼峰） | 手写：`#[sea_orm::model]` + `DeriveEntityModel` + `ActiveModelBehavior` |
| dto | `src/dto/*_dto.rs` | `XxxDto` → `XxxAddDto` / `XxxModifyDto` / `XxxSaveDto` / `XxxQueryDto` | `#[crud_dto]` |
| vo | `src/vo/*_vo.rs` | `XxxVo` / `XxxListVo` / `XxxExVo` | `#[vo]` |
| dao | `src/dao/*_dao.rs` | `XxxDao` | `#[dao(...)]` |
| svc | `src/svc/*_svc.rs` | `XxxSvc` | `#[svc(...)]` |
| ctrl | `src/ctrl/*_ctrl.rs` | `XxxCtrl` | `#[ctrl]` |
| router | `src/router/*_router.rs` | `XxxRouter` | `#[router(crud, routes = [...])]` |
| api_doc | `src/api_doc/*_api_doc.rs` | `XxxApiDoc` | `#[api_doc(add, modify, ...)]` |
| dic | `src/dic/*.rs` | 语义名（`ConnectorStatus`） | `#[dic(default = ...)]` |

**宏按结构体名推导模块名、方法名、路由前缀**，所以后缀不是装饰：

- 必须以 `Svc` / `Ctrl` / `Router` / `ApiDoc` 结尾，且大驼峰；否则宏直接编译报错。
- `XxxYyyRouter` → 前缀取首词小写、其余词用 `-` 连接：`OssBucketRouter` → `/oss/bucket`，
  对应 ctrl 模块名 `oss_bucket_ctrl`。
- **重命名 `XxxRouter` = 改对外 API 路径**，属于破坏性变更，必须在提交信息里写明。

`#[router(crud)]` 生成的固定路由（以 `/oss/bucket` 为例）：

| 方法 | 路径 | handler |
|---|---|---|
| POST | `/oss/bucket` | `add` |
| PUT | `/oss/bucket` | `modify` |
| POST | `/oss/bucket/save` · `/upsert` · `/sync` | `save` · `upsert` · `sync` |
| DELETE | `/oss/bucket/{id}` · `/oss/bucket` | `del_by_id` · `del_by_query_dto` |
| GET | `/oss/bucket/{id}` · `/ex/{id}` | `get_by_id` · `get_ex_by_id` |
| GET | `/oss/bucket` · `/ex` | `get_by_query_dto` · `get_ex_by_query_dto` |
| GET | `/oss/bucket/count` | `count_by_query_dto`（按与列表一致的查询条件统计总数，`extra` 为 U64） |
| GET | `/list` · `/list-ex` · `/page` · `/page-ex` | 列表 / 分页 |
| GET | `/search/{id}` · `/search` · `/search/list` · `/search/page` · `/search/ex/{id}` · `/search/ex` · `/search/list-ex` · `/search/page-ex` | 检索系列 |

`#[api_doc(...)]` 聚合文档地址为 `/{前缀}/{模块路径}/openapi.json`（参数的 handler 标识符
必须与 `#[ctrl]` 生成的方法名一致，漏写就不会进文档）。

### 5.1 新增一张表的落地顺序

顺序不能乱：后面的层按**命名**去找前面的层，名字对不上就直接编译失败。

```rust
// 1) mo：手写 SeaORM 实体（表名 tx_item → 结构体 TxItem）
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, Default)]
#[sea_orm(table_name = "tx_item")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, unique)]
    pub id: i64,
    #[sea_orm(unique)]
    pub code: String,
    pub name: Option<String>,
    pub creater_id: i64,
    pub create_ms: i64,
    pub updater_id: i64,
    pub update_ms: i64,
}
impl ActiveModelBehavior for ActiveModel {}

// 2) dto：只写业务字段 + 注释标签
#[crud_dto]
pub struct TxItemDto {
    /// 编码
    /// @len 1..=50
    pub code: String,
    /// 名称
    /// @len 0..=50
    pub name: Option<String>,
}

// 3) vo：裸 u64，交给宏改写
#[vo]
pub struct TxItemVo {
    /// ID
    pub id: u64,
    /// 编码
    pub code: String,
    // ... 审计字段同样写 u64
}

// 4) dao：把唯一键/外键/模糊列登记进来
#[dao(unique_keys: [("code", "编码")], like_columns: [Column::Code, Column::Name])]
pub struct TxItemDao;

// 5) svc：CRUD 全由宏生成；只有需要钩子时才加参数（见 §9 钩子语义）
#[svc]
pub struct TxItemSvc;

// 6) ctrl / 7) router / 8) api_doc
#[ctrl]
pub struct TxItemCtrl;

#[router(crud, routes = [("/tx/item/echo", get(echo))])]
pub struct TxItemRouter;

#[api_doc(add, modify, get_by_id, page_by_query_dto)]
pub struct TxItemApiDoc;
```

落地检查点：①`mo` 的表名列与结构体名同步；②`dto` 的注释标签与建表 SQL 注释一致；
③`vo` 的 `u64` 不要手挂 serde；④`dao` 的唯一键必须登记；⑤顺序里任何一层名字错一个字，
宏都会找不到对应模块。

## 6. 代码组织与注释风格

**模块文件三段式**（`dao` / `web` / `db` / `api` 全是这个写法，照抄）：

```rust
//! # 模块名
//!
//! 该模块做 X，解决 Y；与 Z 的边界是什么。

mod a;
#[cfg(feature = "db")]
mod b;

pub use a::*;
#[cfg(feature = "db")]
pub use b::*;
```

**函数文档注释**用固定小标题，不要另起格式：

```rust
/// # 一句话说明（动词开头）
///
/// 展开说明「为什么这么设计」「什么情况下不要用」，而不是复述代码。
///
/// ## 参数
/// * `x` - 含义、取值约束
///
/// ## 返回值
/// 返回什么，`None` / `Err` 代表什么
///
/// ## 错误
/// 什么条件下返回哪个错误变体
pub fn foo(x: u32) -> Result<(), DaoError> { /* ... */ }
```

其他约定：

- 注释与文档用**中文**，标识符用**英文**。注释写「取舍与坑」，例如为什么不吞错误、
  为什么不用 `Default` 兜底 —— 现有代码大量这么做，这是本仓库的风格基准。
- 日志用 `tracing`（`info!` / `warn!` / `error!`），**不要 `println!`**。
- 边界函数（DB 错误解析、外部调用、易出错的转换）用 `#[log_call]`；
  支持 `level = trace|debug|info|warn|error`（默认 `debug`）与 `mode = enter`。
  热路径循环里不要加。
- `#[db_unwrap]` / `#[db_unwrap(transaction_required)]` 只用在**带 `db` 参数**的 svc 方法上。
- 全局可变状态用 `ArcSwapOption` / `OnceLock` + 注册函数（见 `db_utils::DB_CONN`、
  `data_scope::set_data_scope`），不要用 `static mut`。

## 7. 错误处理与响应契约

三层错误类型，**逐层 `?` + `From` 上抛，不手工 `map_err` 成字符串**：

```
DaoError（dao/dao_error.rs） → SvcError（svc/svc_error.rs） → CtrlError（web/ctrl/ctrl_error.rs） → Ro
```

关键语义，别弄反：

- `SvcError::NotFound` 是**业务语义**（钩子里「父记录查不到」「字典项没配」也用），
  它**不等于**「这一行不存在」。`upsert` 判断「要不要退化成插入」时只认
  `DaoError::Db(DbErr::RecordNotUpdated)`，即 `SvcError::is_not_found()`。
  把 `NotFound` 当可插入信号 = 静默写脏数据。
- **HTTP 状态码不是错误分类器**：唯一键冲突、插入/删除外键冲突返回 **HTTP 200 +
  `Ro.result = Warn` + 业务码**（`RO_CODE_WARNING_DUPLICATE_KEY` /
  `..._INSERT_VIOLATE_FK` / `..._DELETE_VIOLATE_FK`），前端按 `code` 分支。
  `NotFound` → 404，校验失败 → 400，其余 → 500。
- **非法值一律报错，不静默回落**：`#[dic]` 反序列化、`SeaORM` 取值、`U64` 越界、
  DTO 校验都是这条原则。宁可编译不过 / 返回 400，也不要把脏值伪装成有效值。
- **禁止吞错**：不允许 `let _ = result;`、`.ok()`、`unwrap_or_default()` 掩盖失败。
  确实「尽力而为」的副作用（如刷新广播失败不影响主流程）必须显式写注释说明理由。

统一响应 `Ro<E>`（`robotech/src/api/ro/ro.rs`）：字段固定为
`result` / `msg` / `timestamp` / `extra` / `detail` / `code`，`None` 字段不序列化。
新增业务码加在 `api/ro/ro_code.rs`，并同步前端常量表。

## 8. 对外数据契约

### 8.1 `u64` 一律以**字符串**出 JSON

浏览器 `JSON.parse` 会把超过 `2^53-1` 的数字静默舍入，19 位 idworker id 必坏。

- 有 mo 实体的结构：字段写裸 `u64` / `Option<u64>`，交给 `#[vo]` / `#[crud_dto]` 改写成
  `U64` / `Option<U64>`。**不要**在这些字段上手挂 `#[serde(with = ...)]`。
- 非实体结构（实时计算、聚合、跨服务载荷）：挂 `wheel_rs::serde::{u64_serde, u64_option_serde}`；
  id 列表用 `Vec<U64>`（`wheel_rs` 没有 u64 向量 helper）。
- **禁止**为 u64 再建第三套序列化模块（历史上删过 `api/json_number.rs`）。
- 请求侧对 `u64` 一律发字符串；`tolerant_number` 只作用于 Rust 反序列化，
  救不了浏览器侧已发生的精度丢失。

### 8.2 字典枚举（`#[dic]`）

- HTTP 表示是**整数** `i16`，与库里的 `INT2` 列一致；**不**序列化成变体名。
- 宏自动摘除 `Serialize` / `Deserialize` / `ToSchema` / `Default`，其余 derive 保留。
- **凡是会被 VO 用作字段的字典枚举，必须写 `#[dic(default = 变体名)]`**：
  `#[vo]` 对枚举字段生成 `#[from(~.into())]`，`From` 无法失败，只能靠 `Default` 兜底。
- **列可空 ⇒ VO 字段必须写 `Option<枚举>`**。写非 `Option` 会缺 impl、直接编译失败 ——
  这是故意的缺口，不要为了编过去去加 `From<Option<i16>>`。
- 判别式、DDL 中文注释、PowerDesigner 模型三处必须同步。

### 8.3 VO / DTO

- VO 由宏统一加 `rename_all = "camelCase"`、`skip_serializing_none`、`serde_as`、
  `#[from_owned(Model)]`。**不要**在字段上手写这些属性。
- 批量接口需要脱敏时，用独立的列表 VO（字段用 `robotech::api::mask::*` 包装类型），
  通过 `#[svc(list_vo = ...)]` / `#[ctrl(list_vo = ...)]` 挂上；**单条不脱敏、批量脱敏**。
  `list_vo` 也会作用于 Ex 批量方法（关联字段不再包含），要「脱敏 + 关联」就自己写列表 VO。
- DTO 校验**只认注释标签**，散文不生效：

  | 标签 | 适用 | 作用 |
  |---|---|---|
  | `@len 1..=100` | `String` / `Option<String>` | 字符数（非字节） |
  | `@len 0..=100` | 同上 | 下限 0 = 允许空串 |
  | `@range -90..=90` | 整数 / 浮点（含 `Option`） | 数值区间 |
  | `@check is_id_card_cn` | `String` / `Option<String>` | 语义校验，宏映射到 `robotech::api::validators::id_card_cn` |
  | `@unit ms` | 任意 | 纯标记，不生成校验 |
  | `@secret` | 任意 | 不回显原值（令牌、盐、密钥必标） |

  标签必须与 PowerDesigner 列注释一致，用 `tools/model_comment/sync.py` 同步，
  细节见 `robotech-macros/doc/dto-field-check.md`。
- `#[crud_dto]` 一次生成 `AddDto` / `ModifyDto` / `SaveDto` / `QueryDto` 四类结构体。
  三个字段级标记各管一件事，**别混用**：
  - `#[req_only]`：只进请求、不落库（如树表的 `parent_code`），不进 `QueryDto`；
    漏标会生成 `Column::Xxx` 引用一个不存在的列、编译不过。
  - `#[db_default]`：列在库里有默认值，客户端可不传 —— 只去掉 `required`，区间校验照旧生效。
  - `#[fill_by_svc]`：列非空但由服务端写库前补（如 `tree_code` 由钩子按父编码生成）；
    去掉 `required`，**补值逻辑要在业务钩子里真的实现**。

## 9. 数据库与事务规范

- **表结构基线**：主键 `id` 非自增（由 idworker 生成）；审计四件套
  `creater_id` / `create_ms` / `updater_id` / `update_ms` 非空，补齐规则由 DAO 统一负责：
  - `creater_id` 由请求体传入，**插入时 `updater_id := creater_id`**（新增的更新者就是创建者）；
  - `create_ms` / `update_ms` 缺省**或传 0** 时由服务端 `now_ms()` 补 —— `0` 不是合法时间戳，
    视作「未设置」，否则审计时间会被永久写坏；
  - **更新时 `creater_id` / `create_ms` 被强制置 `NotSet`**：创建者信息不允许被改。
  - 业务侧不要在 svc 里再补一遍时间戳，会与 DAO 的兜底打架。
- **唯一键与外键必须在 `#[dao]` 注册**（`unique_keys` / `foreign_keys`），
  否则报错信息只能退化成约束名或运行时错误（接口 500），业务侧看不到「哪个值重复了」。
- **事务边界**：写方法在 `db = None` 时自开事务（`#[db_unwrap(transaction_required)]`）；
  钩子通过最后一个 `db` 参数拿到**同一事务连接**，级联写必须如此，才能一起提交、一起回滚。
- **钩子语义**（`#[svc]`）：
  - `before_write` / `after_write` 是**无参副作用快捷方式**，被多个服务复用，**不要给它加参数**；
  - `before_<method>` / `after_<method>` 是单方法钩子，优先级高于快捷方式；
  - `after_write` 在**提交之后**触发，只做通知类副作用（如发布刷新信号）；
    它与事务内的 `after_<method>` 是两件事，不能因为配了 `after_add` 就把 `after_write` 吞掉。
- **`sync` 是单向同步**：id 不存在才插入，存在时只有在「本地 `update_ms` < 传入 `update_ms`」
  才改写，否则影响 0 行、不报错。
- **`upsert` 只在「更新未命中」时退化为插入**（判据见 §7），其它失败原样抛出。
- **数据范围注入**：框架只负责把策略产出的条件 AND 到 `#[dao]` 的读写查询；
  `DataScope::condition` 必须**同步、无副作用、不查库**（需要查库的计算在中间件里做完并缓存）。
  未注册策略时全链路透明。更新路径会先按范围校验目标行，**不在范围内直接拒绝**，
  不要通过「按 id 直接 update」绕过列表过滤。
- **排序**：`#[sortable(kind = "flat" | "tree", table = "...", ...)]` 一行接上，
  算法与 SQL 统一在 `robotech/src/dao/sort/`，业务侧不要手写排序值维护。
- **迁移**：`db_migrate!(db_url)` 按库类型选目录 `migrations/{mysql,pgsql,sqlite}`；
  迁移脚本随业务 crate 走，不进框架 crate。
- **全局路由中间件**：用 `ROUTER_LAYER_SLICE` + `route_layer`（不是 `layer`），
  否则未匹配路径会变成 401 而不是 404；多个注册之间**顺序不确定**，有顺序要求就合并成一个注册。

## 10. 测试规范

- 纯函数、正则解析、schema 生成：单元测试放模块内 `#[cfg(test)] mod tests`。
- 集成测试放 `robotech-tests/tests/*.rs`，该 crate `publish = false`，
  `default = ["server"]`（与业务 crate 对齐：`#[crud_dto]` 只在 `server` 下展开）。
- 需要真实环境时用环境变量，**未设置就跳过并打印原因**，不允许 panic、不允许拖垮整个 suite：

  | 变量 | 用途 |
  |---|---|
  | `ROBOTECH_TEST_DB_URL` | PostgreSQL（事务 / 钩子 / upsert / sync） |
  | `ADM_TEST_DB_URL` → 回落 `DATABASE_URL` | 排序用例 |
  | `ROBOTECH_TEST_NATS_URL`（+ `_USER` / `_PASSWORD`） | 刷新信号 / JetStream |

- 测试自建临时 schema 与表，**不读业务库、不依赖现网数据**。
- 断言要钉住**语义**而不是「跑通」：回滚是否真回滚、`after_write` 是否在提交后触发、
  `upsert` 是否真的只在未命中时插入。

## 11. 提交、版本与发布

- 提交信息：`<type>(<scope>): <中文描述>`，type 用
  `feat` / `fix` / `docs` / `refactor` / `test` / `chore`；scope 取 crate 或模块
  （`svc`、`dao`、`web`、`micro-svc`、`template`）。破坏性变更在正文里单列「不兼容」。
- 版本号**三处同步**：`robotech/Cargo.toml`、`robotech-macros/Cargo.toml`、
  `robotech-template/Cargo.toml`；发布提交用 `chore(release): <版本>`。
- `Cargo.lock` 已在 `.gitignore` 内（本仓库现状，不要因此「顺手」删掉或强制加回）。
- 不提交 `target/`、`.DS_Store`、`.idea/`。

## 12. 禁止事项

1. **不手写宏已生成的实现**：`Serialize` / `Deserialize` / `ToSchema` / `Default` /
   `value()` / `from_i16()` / `From` 转换 / `TryGetable` / `ValueType` / CRUD handler /
   路由注册 / DTO 三件套 —— 一律由宏生成，手写会撞实现（E0119）。
2. **不在 `app` / `macros` 里引入** axum、sea-orm、utoipa、reqwest。
3. **不在 `robotech-macros` 里依赖 `robotech`**，也不在宏里做 I/O。
4. **不新增 `robotech::prelude`**：代码里没有这个模块（历史 README 写过 `use robotech::prelude::*`，
   已移除），别照着写；按模块路径 `use robotech::api::...` / `robotech::macros::...` 引用。
5. **不把 `SvcError::NotFound` 当「可插入」信号**。
6. **不为 u64 再建序列化通道**。
7. **不在框架里塞业务语义**（具体表名、字段名、业务码文案、某客户的规则）。
8. **不删除 / 绕过 `[patch.crates-io]`**。
9. **不用 `unwrap()` / `expect()` 处理可恢复错误**；`expect("正则表达式错误")` 这类只允许出现在
   `LazyLock` 静态初始化里。
10. **不用 `println!` 代替 `tracing`**。
11. **不为了「能编译」放宽类型**：`Option<枚举>` 改成 `枚举`、`U64` 改成 `u64`、
    去掉 `default` —— 这些编译错误都是刻意留的护栏，改的是数据契约。

## 13. 交付前自检

```bash
~/.cargo/bin/cargo fmt --all --check                          # 只确认差异范围，不自动修复
~/.cargo/bin/rustfmt --edition 2024 <本次改动的文件>            # 单文件格式化
~/.cargo/bin/cargo clippy -p robotech --all-targets --features app,macros,web,db,refresh
~/.cargo/bin/cargo test -p robotech
~/.cargo/bin/cargo test -p robotech-tests     # 需要 ROBOTECH_TEST_DB_URL（NATS 用例同理）
```

清单：

- [ ] 三种 feature 组合都编过：`app,macros` / `app,macros,web,db` / `app,macros,api-client`
      （最后一条用来证明「不开 db 也能编」）。
- [ ] 本次改动的文件已单文件格式化；提交 diff 里**没有**无关文件的纯格式化改动。
- [ ] `clippy` 无新增告警。
- [ ] 新公开能力已同步：`lib.rs` 模块表 + feature 矩阵、`README.md`、`README_zh.md`。
- [ ] 改了数据契约（u64 / dic / VO 字段 / DTO 标签）→ 同步 `robotech-macros/doc/` 与前端类型。
- [ ] 改了宏 → 补 `robotech-macros/README.md` 的宏表 + `lib.rs` 文档注释里的示例。
- [ ] 改了路由/结构体名 → 确认 API 路径变化，并在提交信息里标注破坏性变更。
- [ ] 新代码没有吞错、没有 `println!`、没有手写宏产物。
</content>
