//! # DAO 元数据实体模块
//!
//! 该模块定义了数据访问层使用的元数据实体：唯一键（`UniqueKey`）与外键（`ForeignKey`）。

mod foreign_key;
mod unique_key;

pub use foreign_key::ForeignKey;
pub use unique_key::UniqueKey;
