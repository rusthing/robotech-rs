use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;

/// # 仅本地访问中间件
///
/// 仅允许来自回环地址（loopback）的请求访问，其他来源一律返回 403 Forbidden。
///
/// ## 返回值
/// 回环地址请求继续执行后续处理，否则返回 403 响应。
pub async fn local_only_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // 统一检查逻辑
    if addr.ip().is_loopback() {
        next.run(request).await
    } else {
        (StatusCode::FORBIDDEN, "Access Denied: Local Only").into_response()
    }
}
