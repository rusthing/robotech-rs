//! # 数据访问对象（DAO）模块
//!
//! 该模块提供数据访问层的公共工具与类型，包括数据库连接与事务操作（`dao_utils`）、
//! 数据访问错误类型（`dao_error`）、唯一键/外键注册表（`unique_keys_utils`、
//! `foreign_keys_utils`）、关联关系转 VO 工具（`belongs_to_utils`）、
//! 无符号整数包装类型（`unsigned_integer_utils`）以及元数据实体（`eo`）。

mod belongs_to_utils;
mod dao_error;
mod dao_utils;
pub mod eo;
mod foreign_keys_utils;
mod unique_keys_utils;
mod unsigned_integer_utils;

pub use belongs_to_utils::*;
pub use dao_error::*;
pub use dao_utils::*;
pub use foreign_keys_utils::*;
pub use unique_keys_utils::*;
pub use unsigned_integer_utils::*;
