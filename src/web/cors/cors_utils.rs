use crate::web::CorsConfig;
use actix_cors::Cors;
use actix_web::http::Method;
use log::debug;
use std::str::FromStr;

pub fn build_cors(cors_config: &Option<CorsConfig>) -> Cors {
    if let Some(cors_config) = cors_config
        && cors_config.enabled
    {
        debug!("构建CORS: {:?}", cors_config);
        let mut cors = Cors::default();

        if let Some(ref allowed_origins) = cors_config.allowed_origins {
            for origin in allowed_origins {
                cors = cors.allowed_origin(origin);
            }
        } else {
            cors = cors.allow_any_origin();
        }

        if let Some(ref allowed_methods) = cors_config.allowed_methods {
            let methods: Result<Vec<Method>, _> = allowed_methods
                .iter()
                .map(|s| Method::from_str(s))
                .collect();

            if let Ok(methods) = methods {
                cors = cors.allowed_methods(methods);
            }
        }

        if let Some(ref allowed_headers) = cors_config.allowed_headers {
            cors = cors.allowed_headers(allowed_headers.iter());
        }

        if let Some(ref exposed_headers) = cors_config.expose_headers {
            cors = cors.expose_headers(exposed_headers.iter());
        }

        if let Some(max_age) = cors_config.max_age {
            cors = cors.max_age(max_age);
        }

        if cors_config.supports_credentials.unwrap_or(false) {
            cors = cors.supports_credentials();
        }

        cors
    } else {
        Cors::permissive()
    }
}
