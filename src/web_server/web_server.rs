use crate::web_server::WebServerSettings;
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

    let listens = web_server_settings.listen.unwrap_or_default();

    // 绑定地址
    if let Some(binds) = web_server_settings.bind {
        for bind in binds {
            server = server
                .bind((bind.clone(), port))
                .expect(&format!("绑定地址失败: {}", bind));
        }
    } else if listens.is_empty() {
        // 如果bind和listen都未配置，默认绑定 "::"
        server = server.bind(("::", port)).expect("绑定地址失败: \"::\"");
    }

    // 监听地址
    for listen in listens {
        let parts: Vec<&str> = listen.rsplitn(2, ':').collect();
        match parts.len() {
            1 => {
                let port: u16 = listen
                    .parse()
                    .expect(&format!("listen的端口解析失败: {}", listen));
                server = server.bind(("::", port)).unwrap();
            }
            2 => {
                let port: u16 = parts[0]
                    .parse()
                    .expect(&format!("listen的端口解析失败: {}", listen));
                let mut bind = parts[1].to_string();
                // 如果是IPv6地址，去除方括号
                if bind.starts_with('[') && bind.ends_with(']') {
                    bind = bind[1..bind.len() - 1].to_string();
                }
                server = server.bind((bind, port)).unwrap();
            }
            _ => panic!("listen格式错误: {}", listen),
        }
    }

    // 获取绑定的地址
    let addrs = server.addrs();
    for addr in addrs {
        info!("服务器监听地址: {}", addr);
    }

    // 启动服务器
    server.run().await.expect("服务器启动失败");
}
