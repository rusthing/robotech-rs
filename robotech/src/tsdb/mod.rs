//! # 时序数据库模块
//!
//! 封装时序数据库（TSDB）客户端能力，目前支持 InfluxDB（`influxdb` 子模块，
//! 需启用 `influxdb` feature）。

#[cfg(feature = "influxdb")]
pub mod influxdb;
