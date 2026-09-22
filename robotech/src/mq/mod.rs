//! # 消息队列模块
//!
//! 封装消息队列（MQ）客户端能力，目前支持 MQTT 协议（`mqtt` 子模块，需启用
//! `mqtt` feature）与 NATS 协议（`nats` 子模块，需启用 `nats` feature）。

#[cfg(feature = "mqtt")]
pub mod mqtt;
#[cfg(feature = "nats")]
pub mod nats;