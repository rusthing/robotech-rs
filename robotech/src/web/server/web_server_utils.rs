use crate::web::cors::build_cors;
use crate::web::server::WebServerConfig;
use crate::web::server::web_server_error::WebServerError;
use actix_web::{App, HttpServer, Responder, get, web};
use libc::pid_t;
use log::{debug, error, info};
use robotech_macros::log_call;
use socket2::{Domain, Socket, Type};
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use tokio::time::timeout;
use tracing::instrument;
use wheel_rs::process::terminate_process;

/// # 健康检查端点
///
/// 提供简单的健康检查接口，返回 "Ok" 字符串表示服务正常运行
///
/// ## 返回值
/// 返回实现了 Responder trait 的响应对象
#[get("/health")]
#[instrument(level = "debug")]
#[log_call]
pub async fn health() -> impl Responder {
    "Ok"
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
pub fn create_reusable_listener(ip: &str, port: u16) -> Result<TcpListener, WebServerError> {
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
pub async fn wait_for_web_server_ready(
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
pub async fn terminate_old_web_server(
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
