use crate::web::{CorsConfig, WebServerError};
use axum::http;
use log::debug;
use std::str::FromStr;
use tower_http::cors::CorsLayer;

pub fn build_cors(cors_config: &Option<CorsConfig>) -> Result<Option<CorsLayer>, WebServerError> {
    if let Some(cors_config) = cors_config
        && cors_config.enabled
    {
        debug!("构建CORS: {:?}", cors_config);
        let mut cors = CorsLayer::default();

        if let Some(ref allowed_origins) = cors_config.allowed_origins {
            for origin in allowed_origins {
                cors = cors.allow_origin(origin.parse::<http::HeaderValue>().map_err(|_| {
                    WebServerError::ParseCors("allowed_origins".to_string(), origin.to_string())
                })?);
            }
        } else {
            cors = cors.allow_origin(tower_http::cors::Any);
        }

        if let Some(ref allowed_methods) = cors_config.allowed_methods {
            let allowed_methods: Result<Vec<http::Method>, _> = allowed_methods
                .iter()
                .map(|s| http::Method::from_str(s))
                .collect();
            cors = cors.allow_methods(allowed_methods.map_err(|e| {
                WebServerError::ParseCors("allowed_methods".to_string(), e.to_string())
            })?);
        } else {
            cors = cors.allow_methods(tower_http::cors::Any);
        }

        if let Some(ref allowed_headers) = cors_config.allowed_headers {
            let allowed_headers: Result<Vec<http::header::HeaderName>, _> = allowed_headers
                .iter()
                .map(|s| http::header::HeaderName::from_str(s))
                .collect();
            cors = cors.allow_headers(allowed_headers.map_err(|e| {
                WebServerError::ParseCors("allowed_headers".to_string(), e.to_string())
            })?);
        } else {
            cors = cors.allow_headers(tower_http::cors::Any);
        }

        if let Some(ref exposed_headers) = cors_config.expose_headers {
            let exposed_headers: Result<Vec<http::header::HeaderName>, _> = exposed_headers
                .iter()
                .map(|s| http::header::HeaderName::from_str(s))
                .collect();
            cors = cors.expose_headers(exposed_headers.map_err(|e| {
                WebServerError::ParseCors("exposed_headers".to_string(), e.to_string())
            })?);
        }

        if let Some(max_age) = cors_config.max_age {
            cors = cors.max_age(max_age);
        }

        if cors_config.allow_credentials.unwrap_or(false) {
            cors = cors.allow_credentials(true);
        }

        Ok(Some(cors))
    } else {
        Ok(None)
    }
}
