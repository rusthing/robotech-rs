//! # robotech 集成测试用的最小服务
//!
//! 这里搭了一套完整的 `mo` / `dto` / `vo` / `dao` / `svc`，只为了在**真实数据库**上
//! 验证框架的事务与钩子语义（见 `tests/tx_integrity.rs`）：
//!
//! - `#[db_unwrap(transaction_required)]` 是否真的提交 / 回滚；
//! - `#[svc]` 的写方法在 `db = None` 时是否自开事务、钩子是否与写在同一个事务里；
//! - `after_write` 是否在**提交之后**才触发（此前是提交前触发，订阅方会读到旧数据）。
//!
//! 表结构不来自任何业务库，测试会在临时 schema 里自己建表。

pub mod dao;
pub mod dto;
pub mod mo;
pub mod probe;
pub mod svc;
pub mod vo;
