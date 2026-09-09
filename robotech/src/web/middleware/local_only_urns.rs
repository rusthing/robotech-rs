use axum::extract::ConnectInfo;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;
use std::sync::Arc;
use wheel_rs::urn_utils::Urn;

/// # 仅本地访问的 URN 状态
///
/// 保存仅允许本地访问的 URN 列表，供 `local_only_urns_middleware` 使用。
#[derive(Clone)]
pub struct LocalOnlyUrnsState {
    pub(crate) local_only_urns: Arc<Vec<Urn>>,
}

/// # 仅本地访问 URN 中间件
///
/// 当请求来自非回环地址，且请求方法与 URI 命中仅本地访问列表中的 URN 时，
/// 返回 403 Forbidden；否则继续执行后续中间件与处理器。
///
/// ## 参数
/// * `state` - 仅本地访问列表状态
/// * `addr` - 客户端地址信息
/// * `request` - 待处理的 HTTP 请求
/// * `next` - 下一个中间件或处理器
///
/// ## 返回值
/// 命中限制规则时返回 403 响应，否则返回后续处理的结果。
pub async fn local_only_urns_middleware(
    State(state): State<LocalOnlyUrnsState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let request_method = request.method().to_string().to_uppercase();
    let request_uri = request.uri().path();

    if !addr.ip().is_loopback()
        && state
            .local_only_urns
            .iter()
            .any(|local_only_urn| local_only_urn.matches(&request_method, request_uri))
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    next.run(request).await
}
