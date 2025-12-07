pub mod ro;
pub mod ro_code;
pub mod ro_result;

// 重新导出结构体，简化外部引用
pub use ro::Ro;
pub use ro_result::RoResult;
