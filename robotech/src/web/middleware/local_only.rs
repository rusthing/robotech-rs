use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;

pub async fn local_only(
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
