//! # 宏（Macros）模块
//!
//! 该模块重新导出 `robotech_macros` 过程宏 crate 中定义的全部宏，
//! 使业务代码可通过 `robotech::macros` 直接使用。

pub use robotech_macros::*;
