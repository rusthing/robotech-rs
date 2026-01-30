use actix_http::body::MessageBody;
use actix_service::{IntoServiceFactory, ServiceFactory};
use actix_web::dev::AppConfig;
use actix_web::{Error, HttpServer};
use log::info;
use rustls::ServerConfig;
use std::fmt::Debug;

pub fn build_https<F, I, S, B>(
    http_server: HttpServer<F, I, S, B>,
    server_config: ServerConfig,
) -> HttpServer<F, I, S, B>
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
        .unwrap()
}
