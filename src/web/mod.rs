pub mod cors;
pub mod ctrl;
pub mod server;

// 重新导出结构体，简化外部引用
pub use cors::cors_settings::CorsSettings;
pub use ctrl::ctrl_error::CtrlError;
pub use ctrl::ctrl_utils;
pub use server::web_server_settings::WebServerSettings;
pub use server::web_server_utils::start_web_server;
