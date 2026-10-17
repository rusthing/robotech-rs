//! # 数据访问对象（DAO）模块
//!
//! 该模块提供数据访问层的公共工具与类型，包括数据库连接与事务操作（`dao_utils`）、
//! 数据访问错误类型（`dao_error`）、唯一键/外键注册表（`unique_keys_utils`、
//! `foreign_keys_utils`）、关联关系转 VO 工具（`belongs_to_utils`）、元数据实体（`eo`）
//! 以及检索参数工具（`bean_searcher_utils`，供 `search_*` 系列方法使用）。
//!
//! ## feature 说明
//!
//! [`bean_searcher_utils`] 分两层：参数层（解析 + 序列化）始终编译，
//! 因此在仅启用 `api-client`（feign 客户端）时也会编译，
//! 使客户端能够以 [`SearchParams`] 作为 `search_*` 方法的查询参数；
//! 翻译层（`to_condition` / `apply_order_by`）需要 sea-orm，由 feature `orm` 门控
//! （`db` 隐含开启 `orm`）。
//! 其余子模块均依赖 `db` feature。

// 检索参数工具：不依赖数据库连接，`api-client` 下同样可用
mod bean_searcher_utils;
#[cfg(feature = "db")]
mod belongs_to_utils;
#[cfg(feature = "db")]
mod dao_error;
#[cfg(feature = "db")]
mod dao_utils;
#[cfg(feature = "db")]
pub mod eo;
#[cfg(feature = "db")]
mod foreign_keys_utils;
#[cfg(feature = "db")]
mod unique_keys_utils;

pub use bean_searcher_utils::*;
#[cfg(feature = "db")]
pub use belongs_to_utils::*;
#[cfg(feature = "db")]
pub use dao_error::*;
#[cfg(feature = "db")]
pub use dao_utils::*;
#[cfg(feature = "db")]
pub use foreign_keys_utils::*;
#[cfg(feature = "db")]
pub use unique_keys_utils::*;
