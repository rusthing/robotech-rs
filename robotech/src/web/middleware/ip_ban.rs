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

/// # IP 拦截状态
///
/// 保存 IP 白名单与黑名单，供 `ip_ban_middleware` 使用。
/// 白名单为空时仅按黑名单拦截；白名单非空时只放行白名单内的来源 IP。
#[derive(Clone)]
pub struct IpBanState {
    pub(crate) ip_white_list: Arc<Vec<IpNet>>,
    pub(crate) ip_black_list: Arc<Vec<IpNet>>,
}

/// # IP 拦截中间件
///
/// 根据白名单/黑名单对请求来源 IP 进行拦截：
/// - 白名单为空：来源 IP 命中黑名单则返回 403
/// - 白名单非空：来源 IP 不在白名单内则返回 403；命中白名单中的精确 IP 直接放行；
///   命中白名单网段后继续检查黑名单，命中则返回 403
///
/// ## 返回值
/// 命中拦截规则时返回 403 响应，否则继续执行后续中间件与处理器。
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
