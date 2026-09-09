//! # Web 属性宏
//!
//! 聚合 `ctrl`、`router`、`api_doc` 三个属性宏实现，分别负责 axum 处理器、
//! 路由注册与 OpenAPI 文档的代码生成。

mod api_doc;
mod ctrl;
mod router;

pub(crate) use api_doc::*;
pub(crate) use ctrl::*;
pub(crate) use router::*;
