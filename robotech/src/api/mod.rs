//! # API 模块
//!
//! 提供统一 API 响应格式与类型工具：
//! - 统一响应对象（`Ro`、`RoResult`、`ro_code`）
//! - 分页响应结构体（`rx`）
//! - `Duration` 包装类型（用于 DTO/VO 与数据库 `String` 类型的互转）
//! - 无符号整数包装类型（`U8`/`U16`/`U32`/`U64`/`U128`，避免 JavaScript 精度丢失）

mod duration;
mod ro;
mod unsigned_integer;

pub use duration::*;
pub use ro::rx::*;
pub use ro::*;
pub use unsigned_integer::*;
