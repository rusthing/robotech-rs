use axum::extract::ConnectInfo;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use ipnet::IpNet;
use std::net::SocketAddr;
use std::sync::Arc;
use wheel_rs::ipnet_utils::is_exact;

#[derive(Clone)]
pub struct IpBanState {
    pub(crate) ip_white_list: Arc<Vec<IpNet>>,
    pub(crate) ip_black_list: Arc<Vec<IpNet>>,
}

pub async fn ip_ban_middleware(
    State(state): State<IpBanState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let src_ip = addr.ip();

    let IpBanState {
        ip_white_list,
        ip_black_list,
    } = state;

    if ip_white_list.is_empty() {
        for ip_net in ip_black_list.iter() {
            if ip_net.contains(&src_ip) {
                return StatusCode::FORBIDDEN.into_response();
            }
        }
    } else {
        let mut is_white = false;
        for ip_net in ip_white_list.iter() {
            if ip_net.contains(&src_ip) {
                if is_exact(ip_net) {
                    return next.run(request).await;
                }
                is_white = true;
                break;
            }
        }
        if !is_white {
            return StatusCode::FORBIDDEN.into_response();
        }
        for ip_net in ip_black_list.iter() {
            if ip_net.contains(&src_ip) {
                return StatusCode::FORBIDDEN.into_response();
            }
        }
    }

    next.run(request).await
}
