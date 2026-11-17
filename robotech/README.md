# RoboTech-RS

[中文](README_zh.md)

RoboTech-RS is a microservice framework written in Rust for backend service development. This project provides core components for RESTful API development, including controller layer, business logic layer, data access layer, and extensive infrastructure tools, significantly simplifying the complexity of developing Rust backend services.

## Project Structure

```
src/
├── api/               # unified response (Ro / PageRx), U64 / F64 / Duration wrappers, validators
├── api_client/        # HTTP client (feature: api-client)
├── app/               # config watcher (AppWatcher), app lifecycle, id-worker setup (feature: app)
├── cache/             # cache abstraction (local-cache / distributed-cache)
├── cfg/               # config building (build_cfg / BaseConfig)
├── cst/               # constants (header names, system operator id)
├── dao/               # data access (feature: db)
│   ├── eo/            # unique-key / foreign-key metadata (NOT business entities)
│   ├── sort/          # server-side ordering (backing #[sortable])
│   ├── data_scope.rs  # data-scope injection
│   └── ...            # transactions, search params, tree codes, relation mapping
├── db/                # pool (setup_db_conn / get_db_conn) and migration (feature: db)
├── env/               # environment info (APP_DIR, binary name, profile)
├── log/               # logging (LogWatcher, local rolling files, mute filter)
├── macros/            # proc-macro re-exports (feature: macros)
├── micro_svc/         # service governance: config center / registry center / Feign
├── mq/                # MQTT / NATS / RabbitMQ
├── redis/             # Redis client
├── refresh/           # refresh signals (JetStream broadcast + subscription self-heal)
├── signal/            # signals and pid file (feature: app)
├── svc/               # service-layer error type (SvcError, feature: app)
├── template/          # call-side wrapper for template rendering
├── tsdb/              # time-series DB (InfluxDB)
└── web/               # web service (feature: web)
    ├── ctrl/          # CtrlError and header helpers
    ├── middleware/    # IP ban, forbidden / local-only URNs
    ├── cors/ https/ health_check/
    └── server/        # axum wrapper, multi-port listeners
```

### Feature Flags Mechanism

This project organizes functional modules through Rust's feature flags mechanism for on-demand loading and dependency optimization:

- **`app`**: Application configuration management, including logging, configuration, environment variables, signal handling
- **`web`**: Web server implementation based on Axum (depends on `app`), supports HTTPS, CORS, middleware
- **`db`**: Database operations based on SeaORM (depends on `app`), provides DAO pattern encapsulation
- **`macros`**: Macro definitions for development simplification
- **`api-client`**: HTTP client wrapper for inter-service communication
- **`micro-svc-client`**: Service-governance clients (unified traits for config center / registry center / hub, plus Consul / Etcd / Nacos implementations) **without axum**, so gateways and MQ consumers can reuse them
- **`config-center`**: Config-center capability (built on `micro-svc-client`), usable standalone
- **`registry-center`**: Registry-center capability (built on `micro-svc-client` + `web`, since instance registration reads the listen port and health-check URL from the web layer)
- **`feign`**: Declarative inter-service calls (built on `api-client` + `registry-center`)

Note: Both `web` and `db` depend on `app` feature, meaning enabling them automatically includes `app` functionality. Both `config-center` and `registry-center` are composed from `micro-svc-client`, so "I only need the config center, without dragging in axum" just enables `config-center`; without `web`, registered instances report `port = 0` and no `health_check_url`.

## Tech Stack

### Core Framework
- **Rust 2024 Edition**: Modern systems programming language ensuring memory safety and zero-cost abstractions
- **Axum**: Ergonomic web framework in Tokio ecosystem, high performance and ergonomic
- **Tokio**: Async runtime providing high-concurrency processing capabilities
- **SeaORM**: Async ORM framework with type safety and compile-time SQL verification

### Configuration & Serialization
- **Serde**: Serialization/deserialization framework
- **Config**: Multi-layer configuration management (files, env vars, CLI args)
- **Validator**: Data validation framework
- **Chrono**: Date and time processing
- **Derive Setters / Typed Builder**: Builder pattern constructors

### Logging & Monitoring
- **Tracing Ecosystem**: Structured logging with JSON output
  - `tracing-subscriber`: Log subscriber and filters
  - `tracing-appender`: Non-blocking writer（滚动文件名由框架内的 `LocalRollingFile` 生成）
  - `tracing-log`: Compatibility with `log` crate
- **Notify**: File system events for config hot-reload

### API Documentation
- **Utoipa**: OpenAPI 3.0 documentation generation
- **Utoipa-Swagger-UI**: Interactive API documentation UI

### Networking & Security
- **Tower / Tower-HTTP**: Middleware collection (CORS, tracing, static files)
- **Tokio-Rustls + AWS-LC-RS**: TLS encryption support
- **IPNet**: IP address and subnet handling
- **Reqwest**: HTTP client

### Database
- **SQLx**: Compile-time SQL verification
- **SeaORM**: Async ORM supporting MySQL and PostgreSQL
- **Regex**: Regular expression matching
- **Once Cell**: Lazy initialization global state

### Utilities
- **Anyhow**: Error handling framework
- **Thiserror**: Custom error types
- **Linkme**: In-memory allocator
- **Nix**: Unix API wrapper
- **Socket2**: Socket API wrapper
- **IdWorker**: Distributed unique ID generation
- **Wheel-RS**: Internal通用 utility library

### Internal Dependencies
- **robotech-macros**: Internal macro library for development acceleration

## Feature Flags

This project uses feature flags to control dependencies and functional modules for on-demand loading and reduced binary size:

### Core Features

| Feature | Description | Dependencies | Default |
|---------|-------------|--------------|----------|
| `app` | App config, logging, env vars, signals | - | ❌ |
| `web` | Web server (includes `app`) | `app`, `axum`, `tower` | ❌ |
| `db` | Database operations (includes `app`) | `app`, `sea-orm`, `sqlx` | ❌ |
| `macros` | Macro definitions | - | ❌ |
| `api-client` | HTTP client | `reqwest` | ❌ |

### Recommended Combinations

**Minimal API Service (smallest combination)**:
```toml
[dependencies.robotech]
version = "1.18.0"
features = ["app"]
```

**Full Web Service + Database**:
```toml
[dependencies.robotech]
version = "1.18.0"
features = ["web", "db", "macros"]
```

**Microservice Scenario (with client)**:
```toml
[dependencies.robotech]
version = "1.18.0"
features = ["web", "db", "macros", "api-client"]
```

## Core Features

### 1. Unified API Response Format

Every response is wrapped in `robotech::api::Ro<E>`; `None` fields are skipped:

```rust
use robotech::api::Ro;

Ro::success("查询成功".to_string())
Ro::success("查询成功".to_string()).detail(Some("2 rows".to_string()))
Ro::illegal_argument("参数不合法".to_string())
Ro::warn("数据已存在".to_string()).code(Some("RTW00001".to_string()))
```

| field | meaning |
|---|---|
| `result` | `RoResult`: `Success = 1` / `IllegalArgument = -1` / `Warn = -2` / `Fail = -3` |
| `msg` | human-readable message |
| `timestamp` | milliseconds, serialized as a **string** (see `U64`) |
| `extra` | payload, `Option<E>` |
| `detail` | optional detail text |
| `code` | optional business code |

Business codes live in `robotech::api::ro_code`: `RTW00001` duplicate key, `RTW00002`
insert foreign-key violation, `RTW00003` delete foreign-key violation.

### 2. Data Access Layer (`#[dao]`)

`#[dao]` generates the whole CRUD + search surface for a SeaORM entity, so the DAO file stays
a one-liner:

```rust
#[dao(
    unique_keys: [("code", "code")],
    like_columns: [Column::Code, Column::Name],
)]
pub struct TxItemDao;
```

Generated methods: `insert` / `update` / `sync` / `delete_by_condition` / `get_by_id` /
`get_by_condition` / `list_by_condition` / `page_by_condition`, plus the `ex_` relation
variants and the `search_*` family. They take `db: &C where C: ConnectionTrait`, so the same
method runs on a pool or inside a transaction.

Registering unique keys and foreign keys is what turns a driver-level constraint violation
into a readable message; unregistered constraints degrade to a raw constraint name or a 500.

### 3. Controller Layer (`#[ctrl]`)

```rust
#[ctrl]
pub struct TxItemCtrl;
```

generates one axum handler per service method (`add` / `modify` / `save` / `upsert` / `sync` /
`del_by_id` / `get_by_id` / `page_by_query_dto` …), each carrying `#[utoipa::path]`,
`#[debug_handler]` and `#[log_call]` and returning `Json<Ro<...>>`. Errors flow through
`CtrlError`:

| error | HTTP |
|---|---|
| validation / bad header / multipart | 400 |
| `SvcError::NotFound` | 404 |
| duplicate key, insert/delete FK violation | **200 + `Ro(Warn)` + `code`** |
| anything else | 500 |

The status code is not the only error taxonomy here — clients branch on `code`.

### 4. Configuration

`bootstrap!(AppConfig)` loads configuration through `AppWatcher`, lowest priority first:

1. `$APP_DIR/config/<binary>.<ext>`, falling back to `$APP_DIR/<binary>.<ext>`
   (`toml` / `json` / `json5` / `yml` / `yaml` / `ini` / `ron`);
2. `<binary>-<profile>.<ext>` layered on top, when `profile` is set;
3. the config center, when `[micro-svc]` is configured;
4. `APP_`-prefixed environment variables, highest priority.

`-c <path>` pins a single file. Editing any watched file re-runs the `setup` callback —
startup and hot reload share one code path, so never put one-off initialization in `main`.

### 5. Logging

`bootstrap!` starts a `LogWatcher` (config from `log.<ext>`, env prefix `LOG`, hot-reloaded).
Application code only uses `tracing`:

```rust
tracing::info!("Service started");
tracing::debug!(user_id = 123, "Processing user");
tracing::error!(error = %e, "Failed to process request");
```

Rotation is handled by the framework's `LocalRollingFile` (by size/time, time-zone aware).
Do not wire your own `tracing_appender` — it would double-write and break the hot reload path.

### 6. Database Connection & Migration

```rust
setup_db_conn(app_config.db.clone(), changed).await?;   // build the pool, reuse if unchanged
let db = get_db_conn()?;                                // global pool
```

`DbConnConfig` wraps SeaORM `ConnectOptions`: pool size, timeouts, SQL logging,
`schema_search_path`. The driver is **not** bundled — enable `sea-orm` with
`sqlx-postgres` / `sqlx-mysql` in your own crate.

Migrations run through `db_migrate!(db_url)`, which selects `migrations/{pgsql,mysql,sqlite}`
from the business crate according to the URL scheme.

### 7. Signal Handling & Graceful Shutdown

`bootstrap!` wires `SignalManager` + `wait_app_exit`; the `-s` flag chooses the behaviour:

| `-s` | behaviour |
|---|---|
| `start` (default) | probe the old process with `SIGCONT`, then start |
| `restart` | `SIGTERM` the old process, then start the new one |
| `stop` / `s` | `SIGTERM`, graceful shutdown |
| `kill` / `k` | `SIGKILL`, force kill |

Shutdown work registers via `register_shutdown_hook`; the pid file sits next to the binary.

### 8. Middleware

Built in: request tracing, CORS, IP ban list, forbidden URNs, local-only URNs, health check
(`/health`).

Business-wide layers go through the `ROUTER_LAYER_SLICE` extension point:

```rust
use axum::{middleware, Router};
use linkme::distributed_slice;
use robotech::web::ROUTER_LAYER_SLICE;

#[distributed_slice(ROUTER_LAYER_SLICE)]
static REQUIRE_IDENTITY_LAYER: fn(Router) -> Router = require_identity_layer;

fn require_identity_layer(router: Router) -> Router {
    // route_layer wraps only registered routes; `layer` would also wrap the fallback and
    // turn unknown paths into 401 instead of 404
    router.route_layer(middleware::from_fn(require_identity))
}
```

The hook runs after all business routers are merged and before health check / Swagger UI, so
probes and docs are never blocked by it. Traversal order across crates is link-order
dependent — merge registrations when the order matters. Per-route layers can still be added
with `tower-http` directly.

### 9. Dictionary Enums (Dic)

**Convention**: every dictionary enum (`*/src/dic/*.rs`) MUST be represented over HTTP as an
**integer** (matching the `INT2` database column), **never** as a variant-name string.
Use the `#[dic]` attribute macro to generate *all* the boilerplate at once; do **not** hand-write
`Serialize` / `Deserialize` / `ToSchema` / `Default` / `value()` / `from_i16()` / `From` impls /
`TryGetable` / `ValueType`.

```rust
use robotech::macros::dic;

// No Default / From<i16> needed: omit the `default` parameter
#[dic]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr, Display, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum ConnectorType {
    Mysql = 0,
    Postgres = 1,
    // ...
}

// Default / From<i16> needed: declare the default variant explicitly
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

**The default must be declared explicitly** — `#[dic]` does **not** treat the first variant as
the default. The leading variant is usually just an accident of declaration order
(`ConnectorStatus` leads with `Running`, yet its business default is `Unassigned`), and a default
value deserves to be an explicit, reviewable decision.

**What `#[dic]` generates** — always:

| Generated impl | Behavior |
|----------------|----------|
| `Serialize` | Serializes to `i16` (HTTP returns `"jobType": 3`) |
| `Deserialize` | Accepts `i16` only; invalid values fail loudly, **never fall back to a default variant** |
| `PartialSchema` | Declares `integer` in OpenAPI, description auto-built as `0=Manual, 1=Cron, ...` |
| `ToSchema` | Schema name = enum name |
| `X::value()` | `const fn`, returns the `i16` discriminant |
| `X::from_i16(i16) -> Option<X>` | Out-of-range returns `None` |
| `From<X> for i16` | Lossless, enum → integer |
| `From<X> for Option<i16>` | Lossless, enum → `Some(integer)` |
| `sea_orm::TryGetable` | `feature = "server"` only; implements `try_get_by` only, **invalid values fail** |
| `sea_orm::sea_query::ValueType` | `feature = "server"` only; accepts `TinyInt`/`SmallInt`, **invalid values fail** |

Only when `#[dic(default = VariantName)]` is present, these are generated **in addition**
(a set of lossy conversions that all need a fallback variant, hence tied together):

| Generated impl | Behavior |
|----------------|----------|
| `Default` | The variant named by `#[dic(default = ...)]` |
| `From<i16> for X` | Falls back to `Default` when out of range |

> **`From<Option<i16>> for X` (`None` → `Default`) is deliberately NOT generated.**
> It is the exact entry point where "a value that had no value becomes a value":
> `#[vo]` emits `#[from(~.map(|v| v.into()))]` for nullable columns (VO field written as
> `Option<Enum>`), which uses only `From<i16>` and keeps `None` as-is; while a non-`Option`
> VO field gets `#[from(~.into())]`, which then lacks the impl **and fails to compile**,
> forcing the author to write `Option<Enum>` in the VO.
> A compile error beats silently explaining a database NULL as some state.
> Compare `dtw_locator_bind.bind_target_type` (`Option<i16>` → `Option<u16>` in the VO).

> **When is `default` mandatory?** `#[vo]` emits `#[from(~.into())]` for enum fields
> (`i16` → enum), and `From` is an infallible contract that cannot report an error — so it can
> only fall back to `Default`. In client mode `#[vo]` additionally requires the field type to
> implement `Default`. Therefore **every dictionary enum used as a VO field must declare
> `default`**, or it will not compile. Enums that only serve the Kafka Connect API and neither
> persist nor appear in a VO (e.g. `ConnectorType`) do not need it.

> **Nullable columns?** Nullable column ⇒ **the VO field must be `Option<Enum>`**:
> an `int2` column + `Option<X>` keeps `None` as `null`; an `int2` column + `X` fails to
> compile (the deliberate gap). Note this guard covers NULL only — `From<i16>` still falls
> back to `Default` for out-of-range dirty values, since `From` cannot fail. Use the lossless
> `from_i16()` when you need to detect dirty data.

**Constraints**:

- The enum must be **non-generic**, have at least one variant, and all variants must be
  **fieldless** (`Fields::Unit`);
- Each variant discriminant must be an **integer literal within `i16`** (when omitted, it is
  `previous + 1`);
- The variant named by `default` must exist, otherwise compilation fails;
- The macro strips `Serialize` / `Deserialize` / `ToSchema` / `Default` from `#[derive(...)]`
  automatically and keeps every other derive (`Debug` / `Clone` / `AsRefStr` / `Display` /
  `EnumString`, ...) untouched;
- A `#[default]` attribute on a variant is ignored (the default can only be set via
  `#[dic(default = ...)]`);
- `AsRefStr` / `Display` / `EnumString` remain for **non-HTTP use** (logging, `value()`, ...);
  their kebab-case form is independent of the HTTP integer form;
- Domain-specific methods (e.g. `AlertStatus::is_ended()`) are still written by hand in a
  separate `impl EnumName { ... }` block and coexist with the generated ones;
- Invalid values **fail loudly** instead of silently falling back to the default variant —
  this prevents dirty database values from masquerading as valid ones.

**Related conventions**:

- The database column is normally `INT2` and the MO field is `i16`; keep the DDL comment,
  the enum discriminants and the PowerDesigner model in sync.
- On the frontend, type enums as numeric literal unions (`type JobType = 0 | 1 | 2 | 3`) and
  map labels with `Record<number, string>` — never match on strings.

## Quick Start

### Prerequisites

- Rust 1.85+ (edition 2024)
- PostgreSQL 12+ or MySQL 8.0+ (if you enable `db`)
- Redis (optional, only for `distributed-cache`)

### Step 1: Create Project

```bash
cargo new my-service
cd my-service
```

### Step 2: Add Dependencies

```toml
# Cargo.toml
[dependencies]
robotech = { version = "1.18.0", features = ["app", "macros", "db", "web"] }
# `bootstrap!` expands to `use clap::Parser` + `#[tokio::main]`
clap = { version = "4", features = ["derive"] }
# the `setup` callback takes `HashMap<String, Value>`
config = "0.15"
# robotech does not pick a database driver for you
sea-orm = { version = "2.0", features = ["sqlx-postgres", "runtime-tokio-rustls"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
tracing = "0.1"
```

### Step 3: Write Code

```rust
// main.rs
use std::collections::HashMap;
use std::sync::Arc;

use config::Value;
use robotech::bootstrap;
use robotech::db::{setup_db_conn, DbConnConfig};
use robotech::web::{setup_web_server, WebServerConfig};
use serde::{Deserialize, Serialize};

/// Application config: `[db]` and `[web]` reuse the framework types
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

// Generates Args + main(): init_env → LogWatcher → init_app → AppWatcher → setup
// → register_app → wait_app_exit
bootstrap!(AppConfig);

/// Runs on startup and on every config change: the name and signature are fixed.
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

### Step 4: Write the Config File

The default name follows the binary: `$APP_DIR/config/<binary>.<ext>` first, then
`$APP_DIR/<binary>.<ext>`. With `profile` set, `<binary>-<profile>.<ext>` is layered on top;
`-c <path>` pins a single file instead.

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

### Step 5: Configure Database

Create the database and user:

```sql
-- PostgreSQL
CREATE USER myuser WITH PASSWORD 'mypassword';
CREATE DATABASE mydb OWNER myuser;

-- MySQL
CREATE USER 'myuser'@'%' IDENTIFIED BY 'mypassword';
CREATE DATABASE mydb CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
GRANT ALL PRIVILEGES ON mydb.* TO 'myuser'@'%';
```

### Step 6: Run Service

```bash
# features belong to the robotech dependency, not to this crate
cargo run

# pin a config file
cargo run -- -c ./config.toml

# override the port, or restart the old process
cargo run -- -p 9840
cargo run -- -s restart

# release
cargo build --release
```

Swagger UI: http://localhost:9840/swagger-ui/ (the `[web] port` above).

## Core Concepts

### Layer map

| layer | content | macro | naming |
|---|---|---|---|
| mo | SeaORM entity | `#[sea_orm::model]` | table `tx_item` → `Model` |
| dto | request objects | `#[crud_dto]` | `XxxDto` → `XxxAddDto` / `XxxModifyDto` / `XxxSaveDto` / `XxxQueryDto` |
| vo | response objects | `#[vo]` | `XxxVo` |
| dao | data access | `#[dao(...)]` | `XxxDao` |
| svc | business logic | `#[svc]` | `XxxSvc` |
| ctrl | HTTP handlers | `#[ctrl]` | `XxxCtrl` |
| router | routes | `#[router(...)]` | `XxxRouter` |
| api_doc | OpenAPI paths | `#[api_doc(...)]` | `XxxApiDoc` |
| dic | dict enums | `#[dic]` | `src/dic/*.rs` |

Each macro finds the previous layer **by deriving its name**, so renaming one struct means
renaming the whole chain.

### Error chain

`DaoError → SvcError → CtrlError → Ro`, mapped once in `#[ctrl]`:

- `SvcError::NotFound` means "the value is gone" (row deleted, relation missing) — not
  "no row returned"; use it only when the caller must know;
- `upsert` only treats `DaoError::Db(DbErr::RecordNotUpdated)` as "nothing to update";
- duplicate key / foreign-key violations return **HTTP 200 with `Ro(Warn)` plus `code`**,
  never a 500.

### IDs and numbers

`u64` is serialized as a **string** so JavaScript never truncates it: entity fields get the
`U64` type from the macro, non-entity structs use `wheel_rs::serde::{u64_serde,
u64_option_serde}`, and ID lists are `Vec<U64>`. Do not invent a third representation.

## Macro System (robotech-macros)

| macro | kind | purpose |
|---|---|---|
| `bootstrap!(T)` | function-like | `Args` + `#[tokio::main] main()`, the full startup chain |
| `db_migrate!(db_url)` | function-like | run `migrations/{pgsql,mysql,sqlite}` by scheme |
| `#[dao(...)]` | attribute | CRUD / search, plus unique-key and FK error translation |
| `#[svc]`, `#[ctrl]`, `#[router(...)]`, `#[api_doc(...)]` | attribute | service, handlers, routes, OpenAPI paths |
| `#[crud_dto]`, `#[vo]` | attribute | request / response objects, `u64 → U64` rewriting |
| `#[dic]` | attribute | dict enums as integers + OpenAPI schema |
| `#[sortable]` | attribute | server-side ordering whitelist |
| `#[db_unwrap]`, `#[log_call]` | attribute | `Ro` error unwrapping, call logging |
| `#[api_client]`, `#[feign]` | attribute | outbound HTTP clients |

Full reference: [robotech-macros](../robotech-macros/)

## Usage Examples

### Layered service: mo → dto → vo → dao → svc → ctrl → router → api_doc

The order matters: each macro looks the previous layer up **by name**. The full walkthrough
with the reasoning is in [AGENTS.md](../AGENTS.md) §5.1.

```rust
// mo: SeaORM entity; table tx_item → struct Model
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, Default)]
#[sea_orm(table_name = "tx_item")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, unique)]
    pub id: i64,
    #[sea_orm(unique)]
    pub code: String,
    // audit columns: creater_id / create_ms / updater_id / update_ms
}

impl ActiveModelBehavior for ActiveModel {}

// dto: generates Add / Modify / Save / Query in one go
#[crud_dto]
pub struct TxItemDto {
    /// Code
    /// @len 1..=50
    pub code: String,
}

// vo: write plain u64; the macro rewrites it to U64 (JSON string, keeps JS precision)
#[vo]
pub struct TxItemVo {
    pub id: u64,
    pub code: String,
}

// dao: registering the unique key is what makes conflicts readable
#[dao(unique_keys: [("code", "code")], like_columns: [Column::Code, Column::Name])]
pub struct TxItemDao;

// svc / ctrl / router / api_doc: fully generated
#[svc]
pub struct TxItemSvc;

#[ctrl]
pub struct TxItemCtrl;

#[router(crud)]
pub struct TxItemRouter;

#[api_doc(add, modify, get_by_id, page_by_query_dto)]
pub struct TxItemApiDoc;
```

`#[router(crud)]` derives its prefix from the struct name: `TxItemRouter` → `/tx/item`, with
`POST` / `PUT` on the root, `POST /save` `/upsert` `/sync`, `DELETE /{id}`, `GET /{id}`,
`/list` `/page` `/ex` `/ex/{id}`, and the whole `search/*` family underneath.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit issues and Pull Requests.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## Contact

- Project Homepage: https://github.com/rusthing/robotech-rs
- Issue Tracker: https://github.com/rusthing/robotech-rs/issues
- Author: zbz