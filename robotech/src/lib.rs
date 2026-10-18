//! # robotech
//!
//! RoboTech 平台的后端服务实现库，为 Web 应用提供 RESTful API 与业务逻辑所需的
//! 基础设施能力，涵盖配置管理、数据库访问、日志、Web 服务、微服务、消息队列等。
//!
//! ## 模块列表
//!
//! - `api_client`：HTTP API 客户端（需启用 feature `api-client`）
//! - `app`：应用级配置与生命周期管理（需启用 feature `app`）
//! - `cache`：缓存抽象（`local-cache` 本地缓存，`distributed-cache` 分布式缓存）
//! - `cfg`：配置构建与解析
//! - `cst`：常量定义
//! - `dao`：数据访问对象（完整能力需启用 feature `db`；仅启用 `api-client` 时只暴露检索参数类型）
//! - `db`：数据库连接管理（需启用 feature `db`）
//! - `env`：环境信息
//! - `log`：日志初始化与热更新
//! - `macros`：过程宏导出（需启用 feature `macros`）
//! - `micro_svc`：微服务能力（注册中心、配置中心、Feign 客户端等）
//! - `mq`：消息队列（MQTT、NATS、RabbitMQ）
//! - `refresh`：刷新信号（基于 JetStream 的变更广播 + 订阅自愈，需启用 feature `refresh`）
//! - `ro`：统一响应对象
//! - `signal`：进程信号与 PID 文件管理（需启用 feature `app`）
//! - `svc`：服务层基础设施（需启用 feature `app`）
//! - `tsdb`：时序数据库
//! - `web`：Web 服务（需启用 feature `web`）
//!
//! ## feature 矩阵
//!
//! 基础能力与可选能力相互正交，按需开启即可，**不会引入用不到的依赖**：
//!
//! | 场景 | features | 引入的关键依赖 |
//! |------|----------|----------------|
//! | 纯后台服务（无 Web / 无 DB，如 MQ 消费者、采集网关） | `app,macros` | 无 axum / sea-orm / utoipa |
//! | 仅需 OpenAPI schema | `app,macros,openapi` | utoipa |
//! | 仅需 sea-orm 类型互操作（`U64` / `Duration`） | `app,macros,orm` | sea-orm |
//! | 数据访问服务 | `app,macros,db` | sea-orm + sqlx |
//! | Web 服务 | `app,macros,web` | axum + utoipa + Swagger UI |
//! | Feign 客户端（`*-api-client`） | `app,macros,api-client`（`feign` 会再带上注册中心） | reqwest + utoipa，**无 sea-orm / sqlx** |
//! | 完整微服务 | `app,macros,db,web,config-center,registry-center` | 全量 |
//!
//! 说明：
//! - `openapi`：`U8`/`U16`/`U32`/`U64`/`U128`、`Duration`、`Ro`、`RoResult`、
//!   `PageRx` 的 `ToSchema` 实现；`#[vo]` / `#[crud_dto]` / `#[dic]` 生成代码依赖它。
//!   `web` 与 `api-client` 会隐含开启。
//! - `orm`：`U64` / `Duration` 与 sea-orm `Value` / `TryGetable` / `ValueType` 的互转，
//!   以及检索参数翻译（`bean_searcher_utils` 的 `to_condition` / `apply_order_by`）
//!   所需的 sea-orm 本体。`db` 会隐含开启；不使用 `db` 但需要该能力的
//!   `*-api` crate（`server` feature）可单独开启 `robotech/orm`。
//!   `api-client` **不**开启 `orm`，因此 feign 客户端只解析/序列化检索参数，不引入 sea-orm。
//!
//! 同时通过 `pub use ro::rx` 重新导出 `rx` 模块（分页响应等）。

pub mod api;
#[cfg(feature = "api-client")]
pub mod api_client;
#[cfg(feature = "app")]
pub mod app;
pub mod cache;
pub mod cfg;
pub mod cst;
// 检索参数类型（`bean_searcher_utils`）不依赖数据库连接，因此在 feign 客户端侧同样需要可用
#[cfg(any(feature = "db", feature = "api-client"))]
pub mod dao;
#[cfg(feature = "db")]
pub mod db;
pub mod env;
pub mod log;
#[cfg(feature = "macros")]
pub mod macros;
pub mod micro_svc;
pub mod mq;
#[cfg(feature = "refresh")]
pub mod refresh;
#[cfg(feature = "redis")]
pub mod redis;
#[cfg(feature = "app")]
pub mod signal;
#[cfg(feature = "app")]
pub mod svc;
pub mod tsdb;
#[cfg(feature = "web")]
pub mod web;