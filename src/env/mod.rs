pub mod env;

// 重新导出结构体，简化外部引用
pub use env::{init_env, Env, ENV};
