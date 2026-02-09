use crate::web::cors::build_cors;
use crate::web::server::web_server_error::WebServerError;
use crate::web::server::WebServerConfig;
use actix_http::body::MessageBody;
use actix_service::{IntoServiceFactory, ServiceFactory};
use actix_web::dev::AppConfig;
use actix_web::middleware::Logger;
use actix_web::{get, web, App, Error, HttpServer, Responder};
use libc::pid_t;
use log::{debug, error, info};
use socket2::{Domain, Socket, Type};
use std::fmt::Debug;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::timeout;
use wheel_rs::process::terminate_process;

/// # 健康检查端点
///
/// 提供简单的健康检查接口，返回 "Ok" 字符串表示服务正常运行
///
/// ## 返回值
/// 返回实现了 Responder trait 的响应对象
#[get("/health")]
async fn health() -> impl Responder {
    "Ok"
}

/// # 启动Web服务器
///
/// 根据配置启动Actix-web服务器，支持多种配置选项包括端口绑定、HTTPS、CORS等
///
/// ## 参数
/// * `web_server_config` - Web服务器配置对象，包含绑定地址、端口、HTTPS、CORS等配置
/// * `configure` - 应用配置函数，用于配置路由和服务
/// * `port_of_args` - 命令行参数指定的端口（可选），优先级高于配置文件
/// * `old_pid` - 旧服务器进程ID（可选），用于重启时停止旧服务
///
/// ## 错误处理
/// * 绑定地址失败时会返回错误
/// * 服务器运行时异常会返回错误
/// * 停止旧服务器失败时会记录警告日志
///
/// ## 使用示例
/// ```rust
/// use crate::web::server::{start_web_server, WebServerConfig};
/// use actix_web::{web, HttpResponse};
///
/// async fn app_config(cfg: &mut web::ServiceConfig) {
///     cfg.route("/", web::get().to(|| async { HttpResponse::Ok().body("Hello World!") }));
/// }
///
/// let config = WebServerConfig::default();
/// start_web_server(config, app_config, None, None).await;
/// ```
pub async fn start_web_server(
    web_server_config: WebServerConfig,
    configure: fn(&mut web::ServiceConfig),
    port_of_args: Option<u16>,
    old_pid: Option<pid_t>,
    app_stated_sender: oneshot::Sender<()>,
) -> Result<(), WebServerError> {
    info!("初始化Web服务器({:?})...", web_server_config);

    let WebServerConfig {
        bind: binds,
        port: mut port_option,
        listen: listens,
        mut reuse_port,
        https: https_config,
        cors: cors_config,
        support_health_check,
        start_wait_timeout,
        start_retry_interval,
        terminate_old_wait_timeout,
        terminate_old_retry_interval,
    } = web_server_config;

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
    if !binds.is_empty() {
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
                    .map_err(|_| WebServerError::ParsePort(listen.to_string()))?;
                if port != 0 {
                    is_random_port = false;
                }
                listen_binds.push(("::".to_string(), port));
            }
            2 => {
                let port: u16 = parts[0]
                    .parse()
                    .map_err(|_| WebServerError::ParsePort(listen.to_string()))?;
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
            _ => Err(WebServerError::ParsePort(listen.to_string()))?,
        }
    }

    // 如果是随机端口，端口复用无意义
    if is_random_port {
        reuse_port = false;
    }

    // 是否支持健康检查
    let support_health_check = is_random_port || reuse_port || support_health_check;

    let mut http_server = HttpServer::new(move || {
        debug!("HttpServer创建worker，并拥有独立的app...");
        // let cors_config = cors_config.clone();
        let mut app = App::new()
            .wrap(Logger::default())
            .wrap(build_cors(&cors_config))
            .configure(configure);

        if support_health_check {
            debug!("支持健康检查");
            app = app.service(health);
        }

        debug!("HttpServer创建worker，并配置完成app.");
        app
    });

    // 如果不是随机端口，且不是复用端口，且是重启服务器，则先停止旧服务器，再启动新服务器
    if !is_random_port && !reuse_port {
        terminate_old_web_server(
            old_pid,
            terminate_old_wait_timeout,
            terminate_old_retry_interval,
        )
        .await?;
    }

    debug!("监听绑定地址...");
    for (bind, port) in &listen_binds {
        if reuse_port {
            debug!("支持端口复用");
            let tcp_listener = create_reusable_listener(bind, *port)?;
            http_server = http_server
                .listen(tcp_listener)
                .map_err(|e| WebServerError::Socket(format!("监听自定义tcp socket失败: {}", e)))?;
        } else {
            http_server = http_server_bind(http_server, bind, *port)?;
        }
    }

    let server = http_server.run();
    tokio::spawn(async move {
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

        if let Err(e) = wait_for_web_server_ready(
            health_url.as_str(),
            start_wait_timeout,
            start_retry_interval,
        )
        .await
        {
            error!("启动Web服务器超时: {}", e);
            return;
        }

        if let Err(_) = app_stated_sender.send(()) {
            error!("发送应用启动完成消息错误");
            return;
        };

        // 如果是随机端口或复用端口，则可以在前面先启动新服务器，后面这里再停止旧服务器
        if is_random_port || reuse_port {
            if let Err(e) = terminate_old_web_server(
                old_pid,
                terminate_old_wait_timeout,
                terminate_old_retry_interval,
            )
            .await
            {
                error!("停止旧Web服务器超时: {}", e);
                return;
            }
        }
    });

    info!("启动Web服务器...");
    server.await?;
    Ok(())
}

/// # 绑定HTTP服务器到指定地址
///
/// 将HTTP服务器绑定到指定的IP地址和端口
///
/// ## 参数
/// * `http_server` - HTTP服务器实例
/// * `ip` - 要绑定的IP地址字符串
/// * `port` - 要绑定的端口号
///
/// ## 泛型参数
/// * `F` - 工厂函数类型
/// * `I` - 服务实例类型
/// * `S` - 服务工厂类型
/// * `B` - 消息体类型
///
/// ## 返回值
/// 返回绑定了地址的HTTP服务器实例
///
/// ## 错误处理
/// 绑定失败时会返回错误信息
fn http_server_bind<F, I, S, B>(
    http_server: HttpServer<F, I, S, B>,
    ip: &str,
    port: u16,
) -> Result<HttpServer<F, I, S, B>, WebServerError>
where
    F: Fn() -> I + Send + Clone + 'static,
    I: IntoServiceFactory<S, actix_http::Request> + 'static,
    S: ServiceFactory<actix_http::Request, Config = AppConfig> + 'static,
    S::Error: Into<Error> + 'static,
    S::InitError: Debug + 'static,
    S::Response: Into<actix_http::Response<B>> + 'static,
    B: MessageBody + 'static,
{
    debug!("绑定地址: [{ip}]:{port}");
    Ok(http_server.bind((ip.to_string(), port)).map_err(|e| {
        WebServerError::Socket(format!("绑定地址失败: {}:{} - {}", ip, port, e).to_string())
    })?)
}

/// # 创建支持端口复用的TCP监听器
///
/// 创建一个支持SO_REUSEADDR和SO_REUSEPORT选项的TCP监听器，用于实现无缝重启
///
/// ## 参数
/// * `ip` - 要监听的IP地址字符串
/// * `port` - 要监听的端口号
///
/// ## 返回值
/// 返回配置好的TcpListener实例
///
/// ## 错误处理
/// * IP地址格式无效时会返回错误
/// * socket创建失败时会返回错误
/// * 设置socket选项失败时会返回错误
/// * 绑定地址失败时会返回错误
/// * 开始监听失败时会返回错误
fn create_reusable_listener(ip: &str, port: u16) -> Result<TcpListener, WebServerError> {
    debug!("创建绑定([{ip}]:{port})可复用端口的监听器...");
    // 解析 IP 地址
    let ip_addr: IpAddr = ip
        .parse()
        .map_err(|_| WebServerError::Socket("无效的 IP 地址格式".to_string()))?;
    let addr: &SocketAddr = &SocketAddr::new(ip_addr, port);
    // 创建 socket
    let socket = Socket::new(
        Domain::for_address(*addr),
        Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .map_err(|e| WebServerError::Socket(format!("创建 socket 失败: {}", e)))?;

    // 设置端口复用选项（关键）
    // SO_REUSEADDR: 允许绑定到处于 TIME_WAIT 状态的地址
    // SO_REUSEPORT: 允许多个进程/线程绑定到同一个端口
    socket
        .set_reuse_address(true)
        .map_err(|e| WebServerError::Socket(format!("设置地址复用选项失败: {}", e)))?;
    socket
        .set_reuse_port(true)
        .map_err(|e| WebServerError::Socket(format!("设置端口复用选项失败: {}", e)))?;

    // 设置非阻塞模式（actix-web 要求）
    socket
        .set_nonblocking(true)
        .map_err(|e| WebServerError::Socket(format!("设置非阻塞模式失败: {}", e)))?;

    // 绑定地址
    socket
        .bind(&(*addr).into())
        .map_err(|e| WebServerError::Socket(format!("绑定地址失败: {}", e)))?;

    // 开始监听（backlog 设置为 1024）
    socket
        .listen(1024)
        .map_err(|e| WebServerError::Socket(format!("开始监听失败: {}", e)))?;

    // 转换为标准库的 TcpListener
    Ok(TcpListener::from(socket))
}

/// # 等待Web服务器准备就绪
///
/// 通过健康检查端点轮询等待Web服务器完全启动并准备好接受请求
///
/// ## 参数
/// * `health_url` - 健康检查URL
/// * `wait_timeout` - 最大等待时间
/// * `retry_interval` - 重试间隔时间
///
/// ## 返回值
/// * `Ok(())` - 服务器准备就绪
/// * `Err(String)` - 等待超时或其他错误
///
/// ## 错误处理
/// * 等待超时时返回错误字符串"启动超时"
async fn wait_for_web_server_ready(
    health_url: &str,
    wait_timeout: Duration,
    retry_interval: Duration,
) -> Result<(), WebServerError> {
    let client = reqwest::Client::new();
    timeout(wait_timeout, async move {
        Ok(loop {
            tokio::time::sleep(retry_interval).await;
            if let Ok(response) = client.get(health_url).send().await {
                if response.status().is_success() {
                    info!("Web服务器通过健康检查，启动完成.");
                    break;
                }
            }
        })
    })
    .await
    .map_err(|_| WebServerError::StartWebServerTimeout(health_url.to_string()))?
}

/// # 停止旧的Web服务器
///
/// 向指定PID的旧Web服务器进程发送停止信号
///
/// ## 参数
/// * `old_pid` - 旧服务器进程ID
///
/// ## 返回值
/// * `Ok(())` - 成功发送停止信号
/// * `Err(Box<dyn std::error::Error>)` - 发送失败
///
/// ## 使用示例
/// ```rust
/// use crate::web::server::stop_old_web_server;
/// stop_old_web_server(12345).await?;
/// ```
async fn terminate_old_web_server(
    old_pid: Option<pid_t>,
    wait_timeout: Duration,
    retry_interval: Duration,
) -> Result<(), WebServerError> {
    if let Some(old_pid) = old_pid {
        debug!("停止运行旧的Web服务器...");
        terminate_process(old_pid, wait_timeout, retry_interval).await?;
    }
    Ok(())
}
