use crate::web::{HttpsConfig, WebServerConfig, WebServerError, build_cors, build_https};
use axum::{Router, debug_handler, routing::get};
use linkme::distributed_slice;
use log::{debug, error, info};
use robotech_macros::log_call;
use socket2::{Domain, Socket, Type};
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::sync::RwLock;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tower_http::trace::TraceLayer;
use utoipa::openapi::OpenApi;
use utoipa_swagger_ui::{SwaggerUi, Url};
use wheel_rs::process::terminate_process;

#[distributed_slice]
pub static ROUTER_SLICE: [fn() -> Router];

#[distributed_slice]
pub static API_DOC_SLICE: [fn() -> (Url<'static>, OpenApi)];

static WEB_SERVICE_HANDLES: RwLock<Option<Vec<JoinHandle<()>>>> = RwLock::new(None);
static STOP_WEB_SERVICE_SENDER: RwLock<Option<broadcast::Sender<()>>> = RwLock::new(None);

fn set_web_service_handles(value: Vec<JoinHandle<()>>) -> Result<(), WebServerError> {
    let mut write_lock = WEB_SERVICE_HANDLES
        .write()
        .map_err(|e| WebServerError::SetWebServiceHandles(e.to_string()))?;
    *write_lock = Some(value);
    Ok(())
}

fn take_web_service_handles() -> Result<Option<Vec<JoinHandle<()>>>, WebServerError> {
    let mut write_lock = WEB_SERVICE_HANDLES
        .write()
        .map_err(|e| WebServerError::TakeWebServiceHandles(e.to_string()))?;
    Ok(write_lock.take())
}

fn set_stop_web_service_sender(value: broadcast::Sender<()>) -> Result<(), WebServerError> {
    let mut write_lock = STOP_WEB_SERVICE_SENDER
        .write()
        .map_err(|e| WebServerError::SetWebServiceHandles(e.to_string()))?;
    *write_lock = Some(value);
    Ok(())
}

fn take_stop_web_service_sender() -> Result<Option<broadcast::Sender<()>>, WebServerError> {
    let mut write_lock = STOP_WEB_SERVICE_SENDER
        .write()
        .map_err(|e| WebServerError::TakeWebServiceHandles(e.to_string()))?;
    Ok(write_lock.take())
}

/// # 健康检查端点
///
/// 提供简单的健康检查接口，返回 "Ok" 字符串表示服务正常运行
///
/// ## 返回值
/// 返回实现了 Responder trait 的响应对象
#[debug_handler]
#[log_call]
pub async fn health() -> &'static str {
    "Ok"
}

#[log_call]
pub async fn start_web_server(
    web_server_config: WebServerConfig,
    port_of_args: Option<u16>,
    old_pid: Option<i32>,
) -> Result<(), WebServerError> {
    let WebServerConfig {
        bind: binds,
        port: port_option,
        listen: listens,
        mut reuse_port,
        https: https_config,
        log_enabled,
        cors: cors_config,
        health_check,
        start_wait_timeout,
        start_retry_interval,
        terminate_old_app_wait_timeout,
        terminate_old_app_retry_interval,
    } = web_server_config;

    let health_check_uri = &health_check.uri;

    let (is_random_port, listen_binds) =
        get_listen_binds(port_of_args, binds, port_option, listens)?;
    if listen_binds.is_empty() {
        Err(WebServerError::ParseListenBinds(
            "没有配置监听绑定".to_string(),
        ))?;
    }

    let mut old_web_service_handles = take_web_service_handles()?;
    let stop_old_web_service_sender = take_stop_web_service_sender()?;

    if is_random_port {
        // 如果是随机端口，则不会开启复用端口(无意义)
        reuse_port = false;
    } else if !reuse_port {
        // 如果不是随机端口，且不是复用端口，则先停止旧服务或应用，然后才能启动新的服务
        if let Some(old_pid) = old_pid {
            // 停止旧应用
            terminate_old_app(
                old_pid,
                terminate_old_app_wait_timeout,
                terminate_old_app_retry_interval,
            )
            .await?;
        } else {
            // 停止旧服务
            if let Some(web_service_handles) = old_web_service_handles.take() {
                stop_old_web_service(stop_old_web_service_sender.clone(), web_service_handles)
                    .await?;
            }
        }
    }

    // 初始化路由
    let mut router = Router::new();
    for build_router in ROUTER_SLICE.iter() {
        router = router.merge(build_router());
    }
    // 判断是否支持健康检查
    if health_check.exposed {
        router = router.route(health_check_uri, get(health));
    } else {
        router = router.route(health_check_uri, get(health));
    }

    // 添加日志中间件
    if log_enabled {
        router = router.layer(TraceLayer::new_for_http());
    }
    // 添加CORS中间件
    if let Some(cors_layer) = build_cors(&cors_config)? {
        router = router.layer(cors_layer);
    }
    // 集成 Swagger UI，访问 /swagger-ui 即可查看文档
    let mut api_docs = vec![];
    for init_api_doc in API_DOC_SLICE.iter() {
        api_docs.push(init_api_doc());
    }
    if !api_docs.is_empty() {
        router = router.merge(SwaggerUi::new("/swagger-ui").urls(api_docs));
    }

    // 判断HTTP协议
    let http_protocol = if let Some(https_config) = https_config.clone()
        && https_config.enabled
    {
        "https"
    } else {
        "http"
    };

    // 绑定地址及端口，并启动服务
    let (stop_web_service_sender, stop_web_service_receiver) = broadcast::channel::<()>(1);
    let (domain_url, web_service_handles) = bind_and_start(
        router,
        reuse_port,
        listen_binds,
        http_protocol,
        https_config,
        stop_web_service_receiver,
    )?;

    // 如果没有旧服务，则等待新服务器启动成功
    if old_web_service_handles.is_none() {
        let heath_check_url = format!("{domain_url}/{health_check_uri}");
        wait_for_web_server_ready(
            heath_check_url.as_str(),
            start_wait_timeout,
            start_retry_interval,
        )
        .await?;
    }

    // 如果是随机端口或复用端口，则可以在前面先启动新的服务，后面这里再停止旧的服务或应用
    if is_random_port || reuse_port {
        if let Some(old_pid) = old_pid {
            // 停止旧应用
            terminate_old_app(
                old_pid,
                terminate_old_app_wait_timeout,
                terminate_old_app_retry_interval,
            )
            .await?;
        } else {
            // 停止旧服务
            if let Some(web_service_handles) = old_web_service_handles.take() {
                tokio::spawn({
                    let stop_old_web_service_sender = stop_old_web_service_sender.clone();
                    async move {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        stop_old_web_service(stop_old_web_service_sender, web_service_handles).await
                    }
                });
            }
        }
    }

    set_web_service_handles(web_service_handles)?;
    set_stop_web_service_sender(stop_web_service_sender)?;

    Ok(())
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
#[log_call]
pub fn create_listener(
    mut bind: String,
    port: u16,
    reuse_port: bool,
) -> Result<TcpListener, WebServerError> {
    // 如果是IPv6地址，去除方括号
    if bind.starts_with('[') && bind.ends_with(']') {
        bind = bind[1..bind.len() - 1].to_string();
    }

    // 解析 IP 地址
    let ip_addr: IpAddr = bind
        .parse()
        .map_err(|_| WebServerError::Socket("无效的 IP 地址格式".to_string()))?;
    let addr: &SocketAddr = &SocketAddr::new(ip_addr, port);
    // 创建 socket
    let socket = Socket::new(
        Domain::for_address(*addr),
        Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .map_err(|e| WebServerError::Socket(format!("创建 socket 失败: {e}")))?;

    // 设置端口复用选项（关键）
    // SO_REUSEADDR: 允许绑定到处于 TIME_WAIT 状态的地址
    // SO_REUSEPORT: 允许多个进程/线程绑定到同一个端口
    socket
        .set_reuse_address(true)
        .map_err(|e| WebServerError::Socket(format!("设置地址复用选项失败: {e}")))?;
    socket
        .set_reuse_port(reuse_port)
        .map_err(|e| WebServerError::Socket(format!("设置端口复用选项失败: {e}")))?;

    // 设置非阻塞模式（actix-web 要求）
    socket
        .set_nonblocking(true)
        .map_err(|e| WebServerError::Socket(format!("设置非阻塞模式失败: {e}")))?;

    // 绑定地址
    socket
        .bind(&(*addr).into())
        .map_err(|e| WebServerError::Socket(format!("绑定{addr}失败: {e}")))?;

    // 开始监听（backlog 设置为 1024）
    socket
        .listen(1024)
        .map_err(|e| WebServerError::Socket(format!("开始监听{addr}失败: {e}",)))?;

    // 转换为标准库的 TcpListener
    Ok(TcpListener::from(socket))
}

/// # 等待Web服务器准备就绪
///
/// 通过健康检查端点轮询等待Web服务器完全启动并准备好接受请求
///
/// ## 参数
/// * `health_check_url` - 健康检查URL
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
    health_check_url: &str,
    wait_timeout: Duration,
    retry_interval: Duration,
) -> Result<(), WebServerError> {
    let client = if health_check_url.starts_with("https://") {
        reqwest::Client::builder()
            .danger_accept_invalid_certs(true) // 忽略未认证的证书
            .build()
            .map_err(|e| WebServerError::BuildReqwestClient(e.to_string()))?
    } else {
        reqwest::Client::new()
    };
    timeout(wait_timeout, async move {
        Ok(loop {
            tokio::time::sleep(retry_interval).await;
            if let Ok(response) = client.get(health_check_url).send().await {
                if response.status().is_success() {
                    info!("Web服务器通过健康检查，启动完成.");
                    break;
                }
            }
        })
    })
    .await
    .map_err(|_| WebServerError::StartWebServerTimeout(health_check_url.to_string()))?
}

pub async fn stop_web_service() -> Result<(), WebServerError> {
    if let Some(stop_web_service_sender) = take_stop_web_service_sender()? {
        stop_web_service_sender
            .send(())
            .map_err(|e| WebServerError::StopService(e.to_string()))?;
    }
    if let Some(web_service_handles) = take_web_service_handles()? {
        for web_service_handle in web_service_handles {
            let _ = web_service_handle
                .await
                .map_err(|e| WebServerError::StopService(e.to_string()))?;
        }
    }
    Ok(())
}

pub async fn stop_old_web_service(
    old_sender: Option<broadcast::Sender<()>>,
    old_handles: Vec<JoinHandle<()>>,
) -> Result<(), WebServerError> {
    if let Some(old_sender) = old_sender {
        old_sender
            .send(())
            .map_err(|e| WebServerError::StopService(e.to_string()))?;
    }
    for web_service_handle in old_handles {
        let _ = web_service_handle
            .await
            .map_err(|e| WebServerError::StopService(e.to_string()))?;
    }
    Ok(())
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
async fn terminate_old_app(
    old_pid: i32,
    wait_timeout: Duration,
    retry_interval: Duration,
) -> Result<(), WebServerError> {
    debug!("停止运行旧的Web服务器...");
    terminate_process(old_pid, wait_timeout, retry_interval).await?;
    Ok(())
}

fn get_listen_binds(
    port_of_args: Option<u16>,
    binds: Vec<String>,
    mut port_option: Option<u16>,
    listens: Vec<String>,
) -> Result<(bool, Vec<(String, u16)>), WebServerError> {
    // 如果命令行参数指定了端口，则使用命令行指定的端口
    if port_of_args.is_some() {
        port_option = port_of_args;
    }

    // 根据传入参数初步判断是否随机端口
    let mut is_random_port = true;
    let port = port_option.unwrap_or(0);
    if port != 0 {
        is_random_port = false;
    }

    // 创建监听绑定地址数组
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
                listen_binds.push(("0.0.0.0".to_string(), port));
            }
            2 => {
                let port: u16 = parts[0]
                    .parse()
                    .map_err(|_| WebServerError::ParsePort(listen.to_string()))?;
                if port != 0 {
                    is_random_port = false;
                }
                let bind = parts[1].to_string();
                listen_binds.push((bind, port));
            }
            _ => Err(WebServerError::ParsePort(listen.to_string()))?,
        }
    }
    Ok((is_random_port, listen_binds))
}

#[log_call]
fn bind_and_start(
    router: Router,
    reuse_port: bool,
    listen_binds: Vec<(String, u16)>,
    http_protocol: &str,
    https_config: Option<HttpsConfig>,
    stop_web_service_receiver: broadcast::Receiver<()>,
) -> Result<(String, Vec<JoinHandle<()>>), WebServerError> {
    let mut web_service_handles = Vec::new();
    let mut domain_url = String::new();
    for (bind, port) in listen_binds {
        let tcp_listener = create_listener(bind.to_string(), port, reuse_port)?;
        // 在 serve 之前获取实际端口
        let actual_addr = tcp_listener.local_addr()?;
        let tokio_listener = tokio::net::TcpListener::from_std(tcp_listener)
            .map_err(|e| WebServerError::Socket(format!("转换为tokio listener失败: {:#}", e)))?;

        // 启动服务
        let mut stop_web_service_receiver = stop_web_service_receiver.resubscribe();
        if let Some(https_config) = https_config.clone()
            && https_config.enabled
        {
            let handle = build_https(
                router.clone(),
                tokio_listener,
                stop_web_service_receiver,
                https_config,
            )?;
            web_service_handles.push(handle);
        } else {
            let server =
                axum::serve(tokio_listener, router.clone()).with_graceful_shutdown(async move {
                    let _ = stop_web_service_receiver.recv().await;
                    info!("停止Axum Web服务");
                });
            let handle = tokio::spawn(async move {
                if let Err(e) = server.await {
                    error!("Axum Web服务运行异常: {:#}", e);
                }
            });
            web_service_handles.push(handle);
        }

        let ip = if bind == "0.0.0.0" {
            "127.0.0.1"
        } else if bind == r"[::]" {
            r"[::1]"
        } else {
            &bind
        };

        info!("监听 <{actual_addr}> 成功✅  -> 🌐 {http_protocol}://{ip}:{port}");

        // 设置域名返回给外部用来测试监听是否成功
        domain_url = format!("{http_protocol}://localhost:{port}");
    }
    Ok((domain_url, web_service_handles))
}
