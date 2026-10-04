//! # 响应编码常量模块
//!
//! 定义 API 响应中 `code` 字段使用的业务编码常量，编码格式为 `RTW` + 5 位序号。

/// 警告编码：键重复（违反唯一性约束）
pub const RO_CODE_WARNING_DUPLICATE_KEY: &str = "RTW00001";
/// 警告编码：插入操作违反外键约束
pub const RO_CODE_WARNING_INSERT_VIOLATE_FK: &str = "RTW00002";
/// 警告编码：删除操作违反外键约束
pub const RO_CODE_WARNING_DELETE_VIOLATE_FK: &str = "RTW00003";
