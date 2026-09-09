//! # 响应对象（RO）模块
//!
//! 该模块定义了统一的 API 响应格式，包括响应结果枚举（`RoResult`）、
//! 通用响应结构体（`Ro`）、响应编码常量（`ro_code`）以及分页响应结构体（`rx`）。

mod ro;
mod ro_code;
mod ro_result;
pub mod rx;

// 重新导出结构体，简化外部引用
pub use ro::*;
pub use ro_code::*;
pub use ro_result::*;
