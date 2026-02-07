use actix_http::body::MessageBody;
use actix_service::{IntoServiceFactory, ServiceFactory};
use actix_web::dev::AppConfig;
use actix_web::{Error, HttpServer};
use log::info;
use rustls::ServerConfig;
use std::fmt::Debug;

/// # 构建HTTPS服务器
///
/// 根据提供的TLS配置构建HTTPS服务器实例
///
/// ## 参数
/// * `http_server` - HTTP服务器实例
/// * `server_config` - TLS服务器配置
///
/// ## 泛型参数
/// * `F` - 工厂函数类型
/// * `I` - 服务实例类型
/// * `S` - 服务工厂类型
/// * `B` - 消息体类型
///
/// ## 返回值
/// 返回配置了HTTPS的服务器实例
///
/// ## 错误处理
/// TLS绑定失败时会返回错误
pub fn build_https<F, I, S, B>(
    http_server: HttpServer<F, I, S, B>,
    server_config: ServerConfig,
) -> Result<HttpServer<F, I, S, B>, Box<dyn std::error::Error>>
where
    F: Fn() -> I + Send + Clone + 'static,
    I: IntoServiceFactory<S, actix_http::Request> + 'static,
    S: ServiceFactory<actix_http::Request, Config = AppConfig> + 'static,
    S::Error: Into<Error> + 'static,
    S::InitError: Debug + 'static,
    S::Response: Into<actix_http::Response<B>> + 'static,
    B: MessageBody + 'static,
{
    info!("构建HTTPS: {:?}", server_config);

    http_server
        .bind_rustls_0_23("0.0.0.0:443", server_config)
        .map_err(|e| format!("HTTPS绑定失败: {}", e).into())
}
