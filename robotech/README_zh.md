# RoboTech-RS

[English](README.md)

RoboTech-RS 是一个用 Rust 编写的后端服务微服务框架。本项目提供 RESTful API 开发的核心组件，包括控制器层、业务逻辑层、数据访问层以及丰富的基础设施工具，极大简化 Rust 后端服务的开发复杂度。

## 项目结构

```
src/
├── api/               # 统一响应（Ro / PageRx）、U64 / F64 / Duration 包装、校验适配、脱敏
├── api_client/        # HTTP 客户端（feature: api-client）
├── app/               # 配置监听（AppWatcher）、应用生命周期、ID worker 初始化（feature: app）
├── cache/             # 缓存抽象（local-cache / distributed-cache）
├── cfg/               # 配置构建与解析（build_cfg / BaseConfig）
├── cst/               # 常量（请求头名、系统操作者 ID）
├── dao/               # 数据访问公共能力（feature: db）
│   ├── eo/            # 唯一键 / 外键元数据（不是业务实体）
│   ├── sort/          # 服务端排序（#[sortable] 的实现）
│   ├── data_scope.rs  # 数据范围注入
│   └── ...            # 事务、检索参数、树编码、关联转换
├── db/                # 连接池（setup_db_conn / get_db_conn）与迁移（feature: db）
├── env/               # 环境信息（APP_DIR、可执行文件名、profile）
├── log/               # 日志（LogWatcher、本地滚动文件、静音过滤）
├── macros/            # 过程宏导出（feature: macros）
├── micro_svc/         # 服务治理：配置中心 / 注册中心 / Feign
├── mq/                # MQTT / NATS / RabbitMQ
├── redis/             # Redis 客户端
├── refresh/           # 刷新信号（JetStream 广播 + 订阅自愈）
├── signal/            # 信号与 PID 文件（feature: app）
├── svc/               # 服务层错误类型（SvcError，feature: app）
├── template/          # 模板渲染的调用侧封装
├── tsdb/              # 时序库（InfluxDB）
└── web/               # Web 服务（feature: web）
    ├── ctrl/          # CtrlError 与请求头工具
    ├── middleware/    # IP 黑名单、禁止访问 / 仅本地 URN
    ├── cors/ https/ health_check/
    └── server/        # Axum 封装、多端口监听
```

### Feature Flags 机制

本项目通过 Rust 的 feature flags 机制组织功能模块，实现按需加载、减少依赖：

- **`app`**：应用配置管理，包含日志记录、配置管理、环境变量处理、信号处理
- **`web`**：基于 Axum 实现 Web 服务器功能（依赖 `app`），支持 HTTPS、CORS、中间件
- **`db`**：基于 SeaORM 实现数据库操作功能（依赖 `app`），提供 DAO 模式封装
- **`macros`**：宏定义，简化开发和代码生成
- **`api-client`**：HTTP 客户端封装，用于微服务间通信
- **`micro-svc-client`**：服务治理客户端（配置中心 / 注册中心 / Hub 的统一契约 + Consul / Etcd / Nacos 实现），**不依赖 axum**，供网关、MQ 消费者等非 Web 进程复用
- **`config-center`**：配置中心能力（依赖 `micro-svc-client`），可单独启用
- **`registry-center`**：注册中心能力（依赖 `micro-svc-client` + `web`，实例注册需要 web 层的监听端口与健康检查 URL）
- **`feign`**：声明式服务间调用（依赖 `api-client` + `registry-center`）

注意：`web` 和 `db` 都依赖 `app` feature，即启用它们时会自动包含 `app` 功能。`config-center` 与 `registry-center` 都由 `micro-svc-client` 组合而来，因此「只想用配置中心、不想拖 axum」的场景直接启用 `config-center` 即可；无 `web` 时注册实例的 `port` 记 `0`、不上报 `health_check_url`。

## 技术栈

### 核心框架
- **Rust 2024 Edition**：现代系统编程语言，保证内存安全和零成本抽象
- **Axum**：Tokio 生态的 Ergonomic Web 框架，高性能且易用上
- **Tokio**：异步运行时，提供高并发处理能力
- **SeaORM**：异步 ORM 框架，类型安全、编译期 SQL 验证

### 配置与序列化
- **Serde**：序列化/反序列化框架
- **Config**：多层配置管理（文件、环境变量、命令行参数）
- **Validator**：数据验证框架
- **Chrono**：日期时间处理
- **Derive Setters / Typed Builder**：Builder 模式构建器

### 日志与监控
- **Tracing Ecosystem**：结构化日志，支持 JSON 格式
  - `tracing-subscriber`：日志订阅器和过滤器
  - `tracing-appender`：文件输出、日志轮转
  - `tracing-log`：兼容 `log` crate
- **Notify**：配置文件热重载监听

### API 文档
- **Utoipa**：OpenAPI 3.0 文档生成
- **Utoipa-Swagger-UI**：交互式 API 文档界面

### 网络与安全
- **Tower / Tower-HTTP**：中间件集合（CORS、追踪、静态文件）
- **Tokio-Rustls + AWS-LC-RS**：TLS 加密传输
- **IPNet**：IP 地址和网段处理
- **Reqwest**：HTTP 客户端

### 数据库
- **SQLx**：编译期 SQL 验证
- **SeaORM**：异步 ORM，支持 MySQL 和 PostgreSQL
- **Regex**：正则表达式匹配
- **Once Cell**：懒初始化全局状态

### 工具库
- **Anyhow**：错误处理框架
- **Thiserror**：自定义错误类型
- **Linkme**：堆内内存分配器
- **Nix**：Unix API 封装
- **Socket2**： Socket API 封装
- **IdWorker**：分布式唯一 ID 生成
- **Wheel-RS**：内部通用工具库

### 内部依赖
- **robotech-macros**：内部宏库，提供简化开发的辅助宏

## Feature Flags

本项目使用 feature flags 来控制依赖和功能模块，实现按需加载、减少二进制体积：

### 核心 Features

| Feature | 说明 | 依赖 | 默认 |
|---------|------|------|------|
| `app` | 应用配置、日志、环境变量、信号处理 | - | ❌ |
| `web` | Web 服务器（包含 `app`） | `app`, `axum`, `tower` | ❌ |
| `db` | 数据库操作（包含 `app`） | `app`, `sea-orm`, `sqlx` | ❌ |
| `macros` | 宏定义 | - | ❌ |
| `api-client` | HTTP 客户端 | `reqwest` | ❌ |

### 推荐组合

**仅 API 服务**（最小组合）：
```toml
[dependencies.robotech]
version = "1.18.0"
features = ["app"]
```

**完整 Web 服务 + 数据库**：
```toml
[dependencies.robotech]
version = "1.18.0"
features = ["web", "db", "macros"]
```

**微服务场景（含客户端）**：
```toml
[dependencies.robotech]
version = "1.18.0"
features = ["web", "db", "macros", "api-client"]
```

## 模块说明

### 核心模块

- **`ro`**: API响应对象（Response Object），提供统一格式的响应结构
  - `RO<T>`：泛型响应包装
  - `Result`：结果类型别名
  - `rx`：响应扩展方法

- **`cst`**: 常量定义
  - 全局常量和枚举
  - 业务状态码定义

### 应用模块（feature: app）

- **`cfg`**: 配置管理
  - 多层配置加载（文件、环境变量、命令行）
  - 配置文件热重载监听
  - 配置验证

- **`log`**: 日志系统
  - Tracing 框架集成
  - 结构化 JSON 日志输出
  - 文件轮转和动态级别调整

- **`env`**: 环境变量管理
  - 环境变量读取和验证
  - 默认值处理

- **`signal`**: 信号处理
  - Unix 信号监听（SIGTERM, SIGINT）
  - 优雅停机支持
  - PID 文件管理

- **`svc`**: 服务层
  - `SvcError` 业务错误类型（含 `NotFound`）
  - 事务由 DAO 方法接收 `ConnectionTrait` 透传

- **`app`**: 应用上下文
  - 全局配置存储
  - 应用生命周期管理

### Web 模块（feature: web）

- **`web::server`**: Web 服务器核心
  - Axum 服务器封装
  - 多端口监听
  - HTTPS/TLS 支持

- **`web::ctrl`**: 控制器支撑
  - `CtrlError`：把 `SvcError` 映射成 HTTP 状态 + `Ro`
  - `#[ctrl]` 宏生成的 handler 负责响应包装

- **`web::cors`**: CORS 中间件
  - 自定义跨域规则
  - 预检请求处理

- **`web::middleware`**: 中间件集合
  - TraceLayer：请求追踪和日志
  - `ip_ban`：IP 黑名单（`IpBanState`）
  - `forbidden_urns` / `local_only_urns`：禁止访问 / 仅本地访问

- **`web::health_check`**: 健康检查
  - `/health` 端点实现
  - 服务状态报告

- **`web::https`**: HTTPS 支持
  - TLS 证书加载
  - HTTP/2 协议支持

### 数据库模块（feature: db）

- **`dao`**: 数据访问层
  - `#[dao]` 宏：为实体生成泛型 CRUD 与检索方法
  - `EO`（Entity Object）：数据库实体映射
  - DAO 工具函数
  - 外键和唯一键处理

- **`db`**: 数据库连接
  - 连接池管理
  - 迁移脚本执行
  - SQL 日志记录

### 宏系统（feature: macros）

- **`macros`**: 简化开发的宏
  - `bootstrap!`：启动链（配置加载 + 监听 + 信号）
  - `db_migrate!`：执行迁移
  - `#[log_call]`：自动记录调用日志
  - 其余见 [robotech-macros](../robotech-macros/)
  - 更多见 [robotech-macros](../robotech-macros/)

### API 客户端（feature: api-client）

- **`api_client`**: HTTP 客户端封装
  - 微服务间调用
  - Multipart 上传支持
  - 流式响应处理

## 核心特性

### 1. 统一的 API 响应格式

所有响应都由 `robotech::api::Ro<E>` 包装，`None` 字段不参与序列化：

```rust
use robotech::api::Ro;

Ro::success("查询成功".to_string())
Ro::success("查询成功".to_string()).detail(Some("共 2 条".to_string()))
Ro::illegal_argument("参数不合法".to_string())
Ro::warn("数据已存在".to_string()).code(Some("RTW00001".to_string()))
```

| 字段 | 含义 |
|---|---|
| `result` | `RoResult`：`Success = 1` / `IllegalArgument = -1` / `Warn = -2` / `Fail = -3` |
| `msg` | 提示信息 |
| `timestamp` | 毫秒时间戳，序列化为**字符串**（见 `U64`） |
| `extra` | 业务数据，`Option<E>` |
| `detail` | 可选明细 |
| `code` | 可选业务码 |

业务码常量在 `robotech::api::ro_code`：`RTW00001` 重复键、`RTW00002` 插入违反外键、
`RTW00003` 删除违反外键。

### 2. 数据访问层（`#[dao]`）

`#[dao]` 一次性生成整条 CRUD + 检索链路，DAO 文件通常只有一行：

```rust
#[dao(
    unique_keys: [("code", "编码")],
    like_columns: [Column::Code, Column::Name],
)]
pub struct TxItemDao;
```

生成的方法：`insert` / `update` / `sync` / `delete_by_condition` / `get_by_id` /
`get_by_condition` / `list_by_condition` / `page_by_condition`，以及带关联的 `ex_` 版本
和 `search_*` 系列。它们接收 `db: &C where C: ConnectionTrait`，同一个方法既能跑在连接池
上，也能跑在事务里。

**唯一键、外键必须登记**：登记了才能把驱动层的约束报错翻译成「哪个字段的哪个值重复了」
这样的可读信息；不登记就会退化成约束名甚至 500。

### 3. 控制器层（`#[ctrl]`）

```rust
#[ctrl]
pub struct TxItemCtrl;
```

为每个服务方法生成一个 axum handler（`add` / `modify` / `save` / `upsert` / `sync` /
`del_by_id` / `get_by_id` / `page_by_query_dto` 等），自带 `#[utoipa::path]`、
`#[debug_handler]`、`#[log_call]`，返回 `Json<Ro<...>>`。错误统一走 `CtrlError`：

| 错误 | HTTP |
|---|---|
| 参数校验 / Header 非法 / multipart | 400 |
| `SvcError::NotFound` | 404 |
| 重复键、插入/删除违反外键 | **200 + `Ro(Warn)` + `code`** |
| 其它 | 500 |

也就是说状态码不是唯一的错误分类依据，前端要按 `code` 分支。

### 4. 配置管理

`bootstrap!(AppConfig)` 通过 `AppWatcher` 加载配置，优先级从低到高：

1. `$APP_DIR/config/<可执行文件名>.<ext>`，不存在则回落
   `$APP_DIR/<可执行文件名>.<ext>`（`toml` / `json` / `json5` / `yml` / `yaml` / `ini` /
   `ron`）；
2. 配了 `profile` 时再叠加 `<可执行文件名>-<profile>.<ext>`；
3. 配了 `[micro-svc]` 时叠加配置中心；
4. `APP_` 前缀的环境变量，优先级最高。

`-c <路径>` 指定单份文件后不再按目录查找。任何被监听的文件改动都会重新执行 `setup`
回调 —— 启动与热更新是同一条代码路径，所以**不要**在 `main` 里写一次性的初始化逻辑。

### 5. 日志

`bootstrap!` 会自动启动 `LogWatcher`（配置读 `log.<ext>`，环境变量前缀 `LOG`，改动热更新）。
业务代码只用 `tracing`：

```rust
tracing::info!("Service started");
tracing::debug!(user_id = 123, "Processing user");
tracing::error!(error = %e, "Failed to process request");
```

日志轮转由框架的 `LocalRollingFile` 负责（按大小/时间，带时区）。**不要**自己再接一套
`tracing_appender`，会重复写并破坏热更新链路。

### 6. 数据库连接与迁移

```rust
setup_db_conn(app_config.db.clone(), changed).await?;   // 建池；配置没变则复用
let db = get_db_conn()?;                                // 全局连接池
```

`DbConnConfig` 包装 SeaORM 的 `ConnectOptions`：连接池大小、超时、SQL 日志、
`schema_search_path` 等。**驱动不由框架绑定**：业务 crate 自己要给 `sea-orm` 开
`sqlx-postgres` / `sqlx-mysql`。

迁移用 `db_migrate!(db_url)`，会按 URL scheme 从业务 crate 的
`migrations/{pgsql,mysql,sqlite}` 目录执行。

### 7. 信号处理与优雅停机

`bootstrap!` 已接好 `SignalManager` + `wait_app_exit`，`-s` 参数决定行为：

| `-s` | 行为 |
|---|---|
| `start`（默认） | 先发 `SIGCONT` 探测旧进程，再启动 |
| `restart` | 先 `SIGTERM` 停旧进程，再启动新进程 |
| `stop` / `s` | `SIGTERM`，优雅停机 |
| `kill` / `k` | `SIGKILL`，强制终止 |

停机逻辑用 `register_shutdown_hook` 注册；PID 文件与可执行文件放在一起。

### 8. 中间件

内置：请求追踪、CORS、IP 黑名单、禁止访问 URN、仅本地访问 URN、健康检查（`/health`）。

业务级全局中间件走 `ROUTER_LAYER_SLICE` 扩展点：

```rust
use axum::{middleware, Router};
use linkme::distributed_slice;
use robotech::web::ROUTER_LAYER_SLICE;

#[distributed_slice(ROUTER_LAYER_SLICE)]
static REQUIRE_IDENTITY_LAYER: fn(Router) -> Router = require_identity_layer;

fn require_identity_layer(router: Router) -> Router {
    // route_layer 只包已注册路由；用 layer 会连 fallback 一起包住，未知路径会变成 401
    router.route_layer(middleware::from_fn(require_identity))
}
```

这个钩子跑在所有业务路由合并之后、健康检查 / Swagger UI 之前，所以探针和文档不会被拦；
跨 crate 的注册顺序取决于 link 顺序，有顺序要求就合并成一次注册。单条路由的中间件仍可
直接用 `tower-http` 挂。

### 9. 字典枚举（Dic）

**规范**：所有字典枚举（`*/src/dic/*.rs`）的 HTTP 表示必须是**整数**（对应数据库的
`INT2` 列），**不是**变体名字符串。用 `#[dic]` 属性宏一次性生成全部样板实现，
**禁止**手写 `Serialize` / `Deserialize` / `ToSchema` / `Default` / `value()` / `from_i16()` /
`From` 转换 / `TryGetable` / `ValueType`。

```rust
use robotech::macros::dic;

// 不需要 Default / From<i16>：不写 default 参数
#[dic]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr, Display, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum ConnectorType {
    Mysql = 0,
    Postgres = 1,
    // ...
}

// 需要 Default / From<i16>：显式声明默认变体
#[dic(default = Unassigned)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr, Display, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum ConnectorStatus {
    Running = 0,
    // ...
    Unassigned = 3,
    // ...
}
```

**默认值必须显式声明** —— `#[dic]` **不会**默认把「第一个变体」当成默认值。
因为「第一个变体」往往只是变体书写顺序的巧合（`ConnectorStatus` 的首位是 `Running`，
业务默认值却是 `Unassigned`），默认值应当是显式、可评审的决定。

**`#[dic]` 生成的内容** —— 总是生成：

| 生成的实现 | 行为 |
|-----------|------|
| `Serialize` | 序列化为 `i16`（HTTP 返回 `"jobType": 3`） |
| `Deserialize` | 只接受 `i16`；非法值直接报错，**不回落默认变体** |
| `PartialSchema` | OpenAPI 声明为 `integer`，描述自动生成 `0=Manual, 1=Cron, ...` |
| `ToSchema` | schema 名 = 枚举名 |
| `X::value()` | `const fn`，返回 `i16` 判别式 |
| `X::from_i16(i16) -> Option<X>` | 越界返回 `None` |
| `From<X> for i16` | 无损，枚举 → 整数 |
| `From<X> for Option<i16>` | 无损，枚举 → `Some(整数)` |
| `sea_orm::TryGetable` | 仅 `feature = "server"`；只实现 `try_get_by`，**非法值报错** |
| `sea_orm::sea_query::ValueType` | 仅 `feature = "server"`；`TinyInt`/`SmallInt` 均接受，**非法值报错** |

仅当写了 `#[dic(default = 变体名)]` 时**额外**生成（这一组是「有损转换」，
都需要一个兜底变体，所以绑在一起）：

| 生成的实现 | 行为 |
|-----------|------|
| `Default` | 取 `#[dic(default = 变体名)]` 指定的变体 |
| `From<i16> for X` | 越界回落 `Default` |

> **`From<Option<i16>> for X`（`None → Default`）是故意不生成的。**
> 它正是「本来没值的变成有值」的入口：`#[vo]` 给可空列生成的转换是
> `#[from(~.map(|v| v.into()))]`（VO 字段写 `Option<枚举>`），只用 `From<i16>`，
> `None` 原样保留；而 VO 字段写非 `Option` 时是 `#[from(~.into())]`，源头是
> `Option<i16>` 就缺 impl **直接编译不过**，逼作者把 VO 字段改成 `Option<枚举>`。
> 宁可编译不过，也不要把数据库 NULL 静默解释成某个状态。
> 参照 `dtw_locator_bind.bind_target_type`（`Option<i16>` → VO 写 `Option<u16>`）。

> **什么时候必须写 `default`？** `#[vo]` 对枚举字段生成的是 `#[from(~.into())]`
> （`i16` → 枚举），而 `From` 是无损契约、无法返回错误，只能靠 `Default` 兜底；
> 另外 client 模式下 `#[vo]` 还会要求字段类型实现 `Default`。
> 所以**凡是被 VO 用作字段的字典枚举，都必须写 `default`**，否则编译不过。
> 只用于 Kafka Connect API、不落库也不进 VO 的枚举（如 `ConnectorType`）就不用写。

> **可空列怎么配？** 列可空 ⇒ **VO 字段必须写 `Option<枚举>`**：
> `int2` 列 + `Option<X>` → `None` 保留为 `null`；`int2` 列 + `X` → 编译报错（故意的缺口）。
> 注意这层保护**只管 NULL**，不管越界脏值 —— `From<i16>` 对枚举范围外的整数仍会回落
> `Default`（`From` 契约无法返回错误）。要检测脏值请用无损的 `from_i16()`。

**约束**：

- 枚举必须**非泛型**，至少有一个变体，且所有变体都是**无字段**的（`Fields::Unit`）；
- 每个变体的判别式必须是 **i16 范围内的整数字面量**（省略时按「上一个 + 1」递增）；
- `default` 指定的变体必须存在，否则编译报错；
- 宏会从 `#[derive(...)]` 中自动摘除 `Serialize` / `Deserialize` / `ToSchema` / `Default`，
  其余派生（`Debug` / `Clone` / `AsRefStr` / `Display` / `EnumString` 等）原样保留；
- 变体上的 `#[default]` 属性会被忽略（默认变体只能由 `#[dic(default = ...)]` 决定）；
- `AsRefStr` / `Display` / `EnumString` 保留用于**非 HTTP 场景**（日志、`value()` 等），
  它们的 kebab-case 表示与 HTTP 的整数表示互不影响；
- 枚举自带的**业务语义方法**（如 `AlertStatus::is_ended()`）仍另开
  `impl 枚举名 { ... }` 块手写，与宏生成的方法互不干扰；
- 非法值一律**报错**而不是静默回落默认变体 —— 避免把库里的脏值伪装成正常值。

**配套约定**：

- 数据库列一般是 `INT2`，MO 里写 `i16`；DDL 中文注释、字典枚举文件的判别式、
  PowerDesigner 模型三处保持同步。
- 前端类型用数字字面量联合（`type JobType = 0 | 1 | 2 | 3`），
  映射表用 `Record<number, string>`，不要按字符串匹配。

## 快速开始

### 先决条件

- Rust 1.85+（edition 2024）
- PostgreSQL 12+ 或 MySQL 8.0+（启用 `db` 时）
- Redis（可选，仅 `distributed-cache` 需要）

### 第一步：创建项目

```bash
cargo new my-service
cd my-service
```

### 第二步：添加依赖

```toml
# Cargo.toml
[dependencies]
robotech = { version = "1.18.0", features = ["app", "macros", "db", "web"] }
# `bootstrap!` 会展开出 `use clap::Parser` 与 `#[tokio::main]`
clap = { version = "4", features = ["derive"] }
# `setup` 回调的参数里要用 `HashMap<String, Value>`
config = "0.15"
# 数据库驱动不由框架绑定，业务 crate 自己选
sea-orm = { version = "2.0", features = ["sqlx-postgres", "runtime-tokio-rustls"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
tracing = "0.1"
```

### 第三步：编写代码

```rust
// main.rs
use std::collections::HashMap;
use std::sync::Arc;

use config::Value;
use robotech::bootstrap;
use robotech::db::{setup_db_conn, DbConnConfig};
use robotech::web::{setup_web_server, WebServerConfig};
use serde::{Deserialize, Serialize};

/// 应用配置：`[db]`、`[web]` 两段直接复用框架提供的类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppConfig {
    pub db: DbConnConfig,
    pub web: WebServerConfig,
    pub mysvc: MySvcConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MySvcConfig {
    pub greeting: String,
}

// 生成 Args 与 main()：init_env → LogWatcher → init_app → AppWatcher → setup
// → register_app → wait_app_exit
bootstrap!(AppConfig);

/// 首次启动与每次配置变更都会调用：函数名和签名都是固定的
async fn setup(
    app_config: &Arc<AppConfig>,
    changed: &Option<HashMap<String, Value>>,
    port: Option<u16>,
    old_pid: Option<u32>,
) -> Result<(), anyhow::Error> {
    setup_db_conn(app_config.db.clone(), changed).await?;
    setup_web_server(app_config.web.clone(), port, old_pid, changed).await?;
    tracing::info!(greeting = %app_config.mysvc.greeting, "service ready");
    Ok(())
}
```

### 第四步：写配置文件

默认文件名跟可执行文件名一致：优先 `$APP_DIR/config/<可执行文件名>.<ext>`，其次
`$APP_DIR/<可执行文件名>.<ext>`。配了 `profile` 时还会叠加
`<可执行文件名>-<profile>.<ext>`；用 `-c <路径>` 指定单份文件后不再按目录查找。

```toml
# config/my-service.toml
app-name = "my-service"
profile = "dev"

[db]
url = "postgres://myuser:mypassword@127.0.0.1:5432/mydb"

[web]
port = 9840

[mysvc]
greeting = "hello"
```

### 第五步：配置数据库

创建数据库与用户：

```sql
-- PostgreSQL
CREATE USER myuser WITH PASSWORD 'mypassword';
CREATE DATABASE mydb OWNER myuser;

-- MySQL
CREATE USER 'myuser'@'%' IDENTIFIED BY 'mypassword';
CREATE DATABASE mydb CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
GRANT ALL PRIVILEGES ON mydb.* TO 'myuser'@'%';
```

### 第六步：运行服务

```bash
# feature 挂在 robotech 依赖上，不在本 crate 上
cargo run

# 指定配置文件
cargo run -- -c ./config.toml

# 覆盖端口 / 重启旧进程
cargo run -- -p 9840
cargo run -- -s restart

# 发布
cargo build --release
```

访问 Swagger UI：http://localhost:9840/swagger-ui/（即上面 `[web] port`）。

## 核心概念

### 分层表

| 层 | 内容 | 宏 | 命名 |
|---|---|---|---|
| mo | SeaORM 实体 | `#[sea_orm::model]` | 表 `tx_item` → `Model` |
| dto | 请求对象 | `#[crud_dto]` | `XxxDto` → `XxxAddDto` / `XxxModifyDto` / `XxxSaveDto` / `XxxQueryDto` |
| vo | 响应对象 | `#[vo]` | `XxxVo` |
| dao | 数据访问 | `#[dao(...)]` | `XxxDao` |
| svc | 业务逻辑 | `#[svc]` | `XxxSvc` |
| ctrl | HTTP 处理 | `#[ctrl]` | `XxxCtrl` |
| router | 路由 | `#[router(...)]` | `XxxRouter` |
| api_doc | OpenAPI | `#[api_doc(...)]` | `XxxApiDoc` |
| dic | 字典枚举 | `#[dic]` | `src/dic/*.rs` |

每个宏都**靠推导名字**去找上一层，所以改一个结构体名就要顺着改完整条链。

### 错误链

`DaoError → SvcError → CtrlError → Ro`，只在 `#[ctrl]` 里集中映射一次：

- `SvcError::NotFound` 表示「这个值没了」（行被删、关联不存在），不是「查不到一行」；
  只有在调用方确实需要知道时才用；
- `upsert` 只把 `DaoError::Db(DbErr::RecordNotUpdated)` 当作「没有需要更新的行」；
- 唯一键 / 外键冲突一律返回 **HTTP 200 + `Ro(Warn)` + `code`**，不是 500。

### ID 与数值编码

`u64` 一律序列化为**字符串**，避免 JavaScript 截断：实体字段由宏改成 `U64`，非实体结构用
`wheel_rs::serde::{u64_serde, u64_option_serde}`，ID 列表用 `Vec<U64>`。不要自己再造第三套
写法。

## 宏系统（robotech-macros）

| 宏 | 形式 | 作用 |
|---|---|---|
| `bootstrap!(T)` | 函数式 | 生成 `Args` 与 `#[tokio::main] main()`，串起完整启动链 |
| `db_migrate!(db_url)` | 函数式 | 按 scheme 执行 `migrations/{pgsql,mysql,sqlite}` |
| `#[dao(...)]` | 属性 | CRUD / 检索，以及唯一键、外键报错翻译 |
| `#[svc]`、`#[ctrl]`、`#[router(...)]`、`#[api_doc(...)]` | 属性 | 服务层、handler、路由、OpenAPI |
| `#[crud_dto]`、`#[vo]` | 属性 | 请求 / 响应对象，`u64 → U64` 改写 |
| `#[dic]` | 属性 | 字典枚举按整数序列化 + OpenAPI schema |
| `#[sortable]` | 属性 | 服务端排序白名单 |
| `#[db_unwrap]`、`#[log_call]` | 属性 | 错误转 `Ro`、调用日志 |
| `#[api_client]`、`#[feign]` | 属性 | 出站 HTTP 客户端 |

完整文档：[robotech-macros](../robotech-macros/)

## 许可证

本项目根据 MIT 许可证授权 - 查看 [LICENSE](LICENSE) 文件了解更多详情。

## 贡献指南

欢迎提交 Issue 和 Pull Request！

1. Fork 本仓库
2. 创建特性分支（`git checkout -b feature/AmazingFeature`）
3. 提交更改（`git commit -m 'Add some AmazingFeature'`）
4. 推送到分支（`git push origin feature/AmazingFeature`）
5. 开启 Pull Request

## 联系方式

- 项目主页：https://github.com/rusthing/robotech-rs
- 问题反馈：https://github.com/rusthing/robotech-rs/issues
- 作者：zbz