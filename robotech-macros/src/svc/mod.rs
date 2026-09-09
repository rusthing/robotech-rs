//! # 服务层属性宏
//!
//! 聚合 `db_unwrap` 与 `svc` 两个属性宏实现，分别负责 Service 方法的数据
//! 连接/事务包装与标准 CRUD 方法生成。

mod db_unwrap;
mod svc;

pub(crate) use db_unwrap::*;
pub(crate) use svc::*;
