use crate::web::cors::build_cors;
use crate::web::server::WebServerConfig;
use actix_http::body::MessageBody;
use actix_service::{IntoServiceFactory, ServiceFactory};
use actix_web::dev::{AppConfig, ServerHandle};
use actix_web::middleware::Logger;
use actix_web::{get, web, App, Error, HttpServer, Responder};
use log::{debug, info};
use socket2::{Domain, Socket, Type};
use std::fmt::Debug;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

static SERVER_HANDLE: RwLock<Option<ServerHandle>> = RwLock::new(None);

#[get("/health")]
async fn health() -> impl Responder {
    "Ok"
}

/// 启动Web服务器
pub async fn start_web_server(
    web_server_config: WebServerConfig,
    configure: fn(&mut web::ServiceConfig),
    port_of_args: Option<u16>,
) {
    info!("初始化Web服务器({:?})...", web_server_config);

    let mut port_option = web_server_config.port;
    let listens = web_server_config.listen.unwrap_or_default();
    let mut reuse_port = web_server_config.reuse_port;
    let https_config = web_server_config.https;
    let cors_config = Arc::new(web_server_config.cors);

    // 如果命令行参数指定了端口，则使用命令行指定的端口
    if port_of_args.is_some() {
        port_option = port_of_args;
    }

    // 是否随机端口
    let mut is_random_port = true;
    let port = port_option.unwrap_or(0);
    if port != 0 {
        is_random_port = false;
    }

    let mut listen_binds = vec![];
    // 解析绑定地址
    if let Some(binds) = web_server_config.bind {
        for bind in binds {
            listen_binds.push((bind, port));
        }
    } else if listens.is_empty() {
        // 如果bind和listen都未配置，默认绑定 "0.0.0.0"
        listen_binds.push(("0.0.0.0".to_string(), port));
    }

    // 解析监听地址
    for listen in &listens {
        // 解析地址，从右侧开始分割，最多产生2部分，可以支持IPv4和IPv6，parts[0]为端口，parts[1]为IP地址
        let parts: Vec<&str> = listen.rsplitn(2, ':').collect();
        match parts.len() {
            1 => {
                let port: u16 = listen
                    .parse()
                    .expect(&format!("listen的端口解析失败: {}", listen));
                if port != 0 {
                    is_random_port = false;
                }
                listen_binds.push(("::".to_string(), port));
            }
            2 => {
                let port: u16 = parts[0]
                    .parse()
                    .expect(&format!("listen的端口解析失败: {}", listen));
                if port != 0 {
                    is_random_port = false;
                }
                let mut bind = parts[1].to_string();
                // 如果是IPv6地址，去除方括号
                if bind.starts_with('[') && bind.ends_with(']') {
                    bind = bind[1..bind.len() - 1].to_string();
                }
                listen_binds.push((bind, port));
            }
            _ => panic!("listen格式错误: {}", listen),
        }
    }

    if is_random_port {
        reuse_port = false;
    }

    // 是否支持健康检查
    let support_health_check =
        is_random_port || reuse_port || web_server_config.support_health_check;

    let mut http_server = HttpServer::new(move || {
        debug!("HttpServer创建worker，并拥有独立的app...");
        let cors_config = cors_config.clone();
        let mut app = App::new()
            .wrap(Logger::default())
            .wrap(build_cors(&cors_config))
            .configure(configure);

        if support_health_check {
            info!("支持健康检查");
            app = app.service(health);
        }

        debug!("HttpServer创建worker，并配置完成app.");
        app
    });

    let server = {
        debug!("获取 server_handle 写锁...");
        let mut server_handle_write_lock =
            SERVER_HANDLE.write().expect("获取 server_handle 写锁失败");

        let old_server_handle_option = server_handle_write_lock.take();

        // 如果不是随机端口，且不是复用端口，且是重启服务器，则先停止旧服务器，再启动新服务器
        if !is_random_port
            && !web_server_config.reuse_port
            && let Some(server_handle) = old_server_handle_option.as_ref()
        {
            info!("停止旧服务器...");
            server_handle.stop(true).await;
        }

        info!("启动Web服务器...");
        // 监听绑定地址
        for (bind, port) in &listen_binds {
            if reuse_port {
                info!("支持端口复用");
                let tcp_listener = create_reusable_listener(bind, *port);
                http_server = http_server
                    .listen(tcp_listener)
                    .expect("监听自定义tcp socket失败");
            } else {
                http_server = http_server_bind(http_server, bind, *port);
            }
        }

        let server = http_server.run();
        tokio::spawn(async move {
            let max_duration = Duration::from_secs(10);
            let retry_interval = Duration::from_millis(500);
            let protocol = if let Some(https_config) = https_config
                && https_config.enabled
            {
                "https"
            } else {
                "http"
            };
            let (ip, port) = &listen_binds[0];
            let ip = if ip == "0.0.0.0" {
                "127.0.0.1"
            } else if ip == "::" {
                "[::1]"
            } else {
                &ip
            };
            let health_url = format!("{}://{}:{}/health", protocol, ip, port);
            wait_for_server_ready(&health_url, max_duration, retry_interval).await;
            info!("启动Web服务器完成.");

            // 如果是随机端口或复用端口，且是重启服务器，则先启动新服务器，再停止旧服务器
            if (is_random_port || web_server_config.reuse_port)
                && let Some(server_handle) = old_server_handle_option.as_ref()
            {
                info!("停止旧服务器...");
                server_handle.stop(true).await;
            }
        });

        let new_server_handle = server.handle();
        *server_handle_write_lock = Some(new_server_handle);
        server
    };
    server.await.expect("服务器运行时异常");
}

fn http_server_bind<F, I, S, B>(
    http_server: HttpServer<F, I, S, B>,
    ip: &str,
    port: u16,
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
    info!("绑定地址: [{ip}]:{port}");
    http_server
        .bind((ip.to_string(), port))
        .expect(&format!("绑定地址失败: {}:{}", ip, port))
}

fn create_reusable_listener(ip: &str, port: u16) -> TcpListener {
    info!("创建绑定([{ip}]:{port})可复用端口的监听器...");
    // 解析 IP 地址
    let ip_addr: IpAddr = ip.parse().expect("无效的 IP 地址格式");
    let addr: &SocketAddr = &SocketAddr::new(ip_addr, port);
    // 创建 socket
    let socket = Socket::new(
        Domain::for_address(*addr),
        Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .expect("创建 socket 失败");

    // 设置端口复用选项（关键）
    // SO_REUSEADDR: 允许绑定到处于 TIME_WAIT 状态的地址
    // SO_REUSEPORT: 允许多个进程/线程绑定到同一个端口
    socket
        .set_reuse_address(true)
        .expect("设置地址复用选项失败");
    socket.set_reuse_port(true).expect("设置端口复用选项失败");

    // 设置非阻塞模式（actix-web 要求）
    socket.set_nonblocking(true).expect("设置非阻塞模式失败");

    // 绑定地址
    socket.bind(&(*addr).into()).expect("绑定地址失败");

    // 开始监听（backlog 设置为 1024）
    socket.listen(1024).expect("开始监听失败");

    // 转换为标准库的 TcpListener
    TcpListener::from(socket)
}

async fn wait_for_server_ready(health_url: &str, max_duration: Duration, retry_interval: Duration) {
    let client = reqwest::Client::new();
    let start_time = Instant::now();

    loop {
        if start_time.elapsed() >= max_duration {
            break;
        }

        if let Ok(response) = client.get(health_url).send().await {
            if response.status().is_success() {
                info!("服务器通过健康检查，启动完成.");
                return;
            }
        }

        tokio::time::sleep(retry_interval).await;
    }
    panic!("服务器启动超时.");
}
