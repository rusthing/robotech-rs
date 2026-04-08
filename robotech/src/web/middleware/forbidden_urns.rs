use axum::{
    extract::{Request, State}, // 1. 必须使用 axum::extract::Request
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response}, // 2. 引入 IntoResponse 和 Response
};
use std::sync::Arc;
use wheel_rs::urn_utils::Urn;

type ForbiddenUrns = Arc<Vec<Urn>>;

// 注意：返回值改为了明确的 Response，而不是 Result<Response, StatusCode>
pub async fn forbidden_urns_middleware(
    State(forbidden_urns): State<ForbiddenUrns>,
    request: Request, // 剥离 <Body>，直接使用 axum 原生的 Request
    next: Next,
) -> Response {
    let request_method = request.method().to_string().to_uppercase();
    let request_uri = request.uri().path();

    if forbidden_urns
        .iter()
        .any(|forbidden_urn| forbidden_urn.matches(&request_method, request_uri))
    {
        // 遇到错误时，主动调用 .into_response() 转换为统一的 Response
        return StatusCode::FORBIDDEN.into_response();
    }

    // 正常放行，next.run 的返回值本来就是严格的 Response
    next.run(request).await
}
