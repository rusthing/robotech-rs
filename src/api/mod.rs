pub mod api;
pub mod api_error;
pub mod api_settings;

// 重新导出结构体，简化外部引用
pub use api::CrudApi;
pub use api_error::ApiError;
pub use api_settings::ApiSettings;
