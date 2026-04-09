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

#[derive(Clone)]
pub struct LocalOnlyUrnsState {
    pub(crate) local_only_urns: Arc<Vec<Urn>>,
}

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
