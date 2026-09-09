//! # 消息队列模块
//!
//! 封装消息队列（MQ）客户端能力，目前支持 MQTT 协议（`mqtt` 子模块，需启用
//! `mqtt` feature）。

#[cfg(feature = "mqtt")]
pub mod mqtt;
