use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use wheel_rs::urn_utils::Urn;

/// # 禁止访问的 URN 状态
///
/// 保存禁止访问的 URN 列表，供 `forbidden_urns_middleware` 使用。
#[derive(Clone)]
pub struct ForbiddenUrnsState {
    pub(crate) forbidden_urns: Arc<Vec<Urn>>,
}

/// # 禁止访问 URN 中间件
///
/// 当请求的 HTTP 方法与 URI 命中禁止列表中的任意 URN 时，直接返回 403 Forbidden；
/// 否则继续执行后续中间件与处理器。
///
/// ## 参数
/// * `state` - 禁止访问列表状态
/// * `request` - 待处理的 HTTP 请求
/// * `next` - 下一个中间件或处理器
///
/// ## 返回值
/// 命中禁止列表时返回 403 响应，否则返回后续处理的结果。
pub async fn forbidden_urns_middleware(
    State(state): State<ForbiddenUrnsState>,
    request: Request,
    next: Next,
) -> Response {
    let request_method = request.method().to_string().to_uppercase();
    let request_uri = request.uri().path();

    if state
        .forbidden_urns
        .iter()
        .any(|forbidden_urn| forbidden_urn.matches(&request_method, request_uri))
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    next.run(request).await
}
