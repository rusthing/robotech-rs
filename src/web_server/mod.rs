pub mod web_server;
pub mod web_server_settings;

// 重新导出结构体，简化外部引用
pub use web_server::start_web_server;
pub use web_server_settings::WebServerSettings;
