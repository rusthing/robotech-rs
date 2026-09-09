//! # 服务层（SVC）模块
//!
//! 该模块定义了服务层的公共错误类型（`SvcError`），用于统一表达与传播
//! 服务层可能遇到的各种异常情况。

mod svc_error;
pub use svc_error::*;
