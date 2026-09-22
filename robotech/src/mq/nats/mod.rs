//! # NATS 消息模块
//!
//! 基于 `async-nats` 提供 NATS 消息系统的连接、发布与订阅能力，支持
//! Core NATS（Publish/Subscribe）与 JetStream（持久化消息、Push Consumer）。
//!
//! ## 模块说明
//!
//! - [`nats_config`] — NATS 连接配置与全局客户端管理
//! - [`nats_error`] — NATS 操作错误类型定义
//! - [`nats_utils`] — 连接初始化、消息发布、订阅等工具函数

mod nats_config;
mod nats_error;
mod nats_utils;

pub use nats_config::*;
pub use nats_error::*;
pub use nats_utils::*;