//! # RabbitMQ 消息模块
//!
//! 基于 `lapin` 提供 RabbitMQ 消息系统的连接、发布与消费能力，支持
//! Exchange 声明、Queue 声明与绑定、消息发布与消费。
//!
//! ## 模块说明
//!
//! - [`rabbitmq_config`] — RabbitMQ 连接配置与全局客户端管理
//! - [`rabbitmq_error`] — RabbitMQ 操作错误类型定义
//! - [`rabbitmq_utils`] — 连接初始化、Exchange/Queue 声明、消息发布、消费等工具函数

mod rabbitmq_config;
mod rabbitmq_error;
mod rabbitmq_utils;

pub use rabbitmq_config::*;
pub use rabbitmq_error::*;
pub use rabbitmq_utils::*;