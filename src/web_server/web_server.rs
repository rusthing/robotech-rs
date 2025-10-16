use crate::settings::WebServerSettings;
use actix_web::middleware::Logger;
use actix_web::{App, HttpServer};
use log::info;

pub async fn start_web_server(
    web_server_settings: WebServerSettings,
    configure: fn(&mut actix_web::web::ServiceConfig),
) {
    info!("创建Web服务器({:?})并运行...", web_server_settings);

    let port = web_server_settings.port.unwrap();
    let mut server =
        HttpServer::new(move || App::new().wrap(Logger::default()).configure(configure));

    // 绑定IP地址
    for bind in web_server_settings.bind {
        server = server.bind((bind, port)).unwrap();
    }

    // 启动服务器
    server.run().await.expect("服务器启动失败");
}
