//! # API 模块
//!
//! 提供统一 API 响应格式与类型工具：
//! - 统一响应对象（`Ro`、`RoResult`、`ro_code`）
//! - 分页响应结构体（`rx`）
//! - `Duration` 包装类型（用于 DTO/VO 与数据库 `String` 类型的互转）
//! - `F64` 包装类型（用于 DTO/VO 与数据库 `NUMERIC`/`DECIMAL` 列的互转）
//! - 无符号整数包装类型（`U8`/`U16`/`U32`/`U64`/`U128`，避免 JavaScript 精度丢失）
//! - `tolerant_number`：原生数值字段「字符串与数字都收」的 `deserialize_with` 助手
//! - `validators`：字段格式校验适配，`#[crud_dto]` 的 `@check` 标签指向这里
//! - `mask`：展示脱敏包装类型（列表 VO 的敏感字段用 `MaskedMobile` 等，出口遮蔽）

mod duration;
mod float;
/// 展示脱敏包装类型：列表 VO 的敏感字段用它，序列化与日志出口遮蔽原值
mod mask;
mod ro;
mod unsigned_integer;

pub mod tolerant_number;
/// 字段格式校验适配：`#[crud_dto]` 的 `@check` 标签指向这里（需 `app` feature）
#[cfg(feature = "app")]
pub mod validators;

pub use duration::*;
pub use float::*;
pub use mask::*;
pub use ro::rx::*;
pub use ro::*;
pub use unsigned_integer::*;
