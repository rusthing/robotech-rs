pub mod cors;
pub mod ctrl;
pub mod server;

// 重新导出结构体，简化外部引用
pub use cors::cors_config::CorsConfig;
pub use ctrl::ctrl_error::CtrlError;
pub use ctrl::ctrl_utils;
pub use server::web_server_config::WebServerConfig;
pub use server::web_server_utils::start_web_server;
