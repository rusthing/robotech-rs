//! # Etcd 后端适配器
//!
//! 基于 `etcd-client` crate 实现配置中心（KV + watch）与注册中心（租约注册）能力，
//! 配置 key 规范与注册流程见 `etcd_client.rs` 顶部说明。

mod etcd_client;
mod etcd_config;
/// etcd 连接选项的序列化包装类型。
pub mod etcd_connect_option_wrapper;

pub use etcd_client::*;
pub use etcd_config::*;
