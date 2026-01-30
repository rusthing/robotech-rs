pub mod cors;
pub mod ctrl;
pub mod https;
pub mod server;

// 重新导出结构体，简化外部引用
pub use cors::cors_config::CorsConfig;
pub use cors::cors_utils::build_cors;
pub use ctrl::ctrl_error::CtrlError;
pub use ctrl::ctrl_utils;
pub use https::https_config::HttpsConfig;
pub use https::https_utils::build_https;
pub use server::web_server_config::WebServerConfig;
pub use server::web_server_utils::start_web_server;
