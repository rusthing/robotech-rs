//! # InfluxDB 时序数据库
//!
//! 基于 `influxdb` crate 提供 InfluxDB 客户端构建能力：配置（`InfluxdbConfig`）、
//! 错误类型（`InfluxdbError`）与客户端构建入口（`build_influxdb_client`）。

mod influxdb_config;
mod influxdb_error;
mod influxdb_utils;

pub use influxdb_config::*;
pub use influxdb_error::*;
pub use influxdb_utils::*;
