//! # Consul 后端适配器
//!
//! 通过 Consul 标准 HTTP API（`/v1/kv`、`/v1/agent`、`/v1/health`）实现
//! 配置中心与注册中心能力，不依赖专用 SDK；具体实现见 `consul_client.rs`。

mod consul_client;
mod consul_config;

pub use consul_client::*;
pub use consul_config::*;
