//! # robotech
//!
//! RoboTech 平台的后端服务实现库，为 Web 应用提供 RESTful API 与业务逻辑所需的
//! 基础设施能力，涵盖配置管理、数据库访问、日志、Web 服务、微服务、消息队列等。
//!
//! ## 模块列表
//!
//! - `api_client`：HTTP API 客户端（需启用 feature `api-client`）
//! - `app`：应用级配置与生命周期管理（需启用 feature `app`）
//! - `cfg`：配置构建与解析
//! - `cst`：常量定义
//! - `dao`：数据访问对象（需启用 feature `db`）
//! - `db`：数据库连接管理（需启用 feature `db`）
//! - `env`：环境信息
//! - `log`：日志初始化与热更新
//! - `macros`：过程宏导出（需启用 feature `macros`）
//! - `micro_svc`：微服务能力（注册中心、配置中心、Feign 客户端等）
//! - `mq`：消息队列
//! - `ro`：统一响应对象
//! - `signal`：进程信号与 PID 文件管理（需启用 feature `app`）
//! - `svc`：服务层基础设施（需启用 feature `app`）
//! - `tsdb`：时序数据库
//! - `web`：Web 服务（需启用 feature `web`）
//!
//! 同时通过 `pub use ro::rx` 重新导出 `rx` 模块（分页响应等）。

#[cfg(feature = "api-client")]
pub mod api_client;
#[cfg(feature = "app")]
pub mod app;
pub mod cfg;
pub mod cst;
#[cfg(feature = "db")]
pub mod dao;
#[cfg(feature = "db")]
pub mod db;
pub mod env;
pub mod log;
#[cfg(feature = "macros")]
pub mod macros;
pub mod micro_svc;
pub mod mq;
pub mod ro;
#[cfg(feature = "app")]
pub mod signal;
#[cfg(any(feature = "app"))]
pub mod svc;
pub mod tsdb;
#[cfg(feature = "web")]
pub mod web;

pub use ro::rx;
