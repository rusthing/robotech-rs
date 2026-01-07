use crate::web::CorsSettings;
use actix_cors::Cors;
use actix_web::http::Method;
use log::info;
use std::str::FromStr;

pub fn build_cors(cors_settings: &Option<CorsSettings>) -> Cors {
    info!("初始化CORS: {:?}", cors_settings);

    if let Some(cors_settings) = cors_settings {
        let mut cors = Cors::default();

        if let Some(ref allowed_origins) = cors_settings.allowed_origins {
            for origin in allowed_origins {
                cors = cors.allowed_origin(origin);
            }
        } else {
            cors = cors.allow_any_origin();
        }

        if let Some(ref allowed_methods) = cors_settings.allowed_methods {
            let methods: Result<Vec<Method>, _> = allowed_methods
                .iter()
                .map(|s| Method::from_str(s))
                .collect();

            if let Ok(methods) = methods {
                cors = cors.allowed_methods(methods);
            }
        }

        if let Some(ref allowed_headers) = cors_settings.allowed_headers {
            cors = cors.allowed_headers(allowed_headers.iter());
        }

        if let Some(ref exposed_headers) = cors_settings.expose_headers {
            cors = cors.expose_headers(exposed_headers.iter());
        }

        if let Some(max_age) = cors_settings.max_age {
            cors = cors.max_age(max_age);
        }

        if cors_settings.supports_credentials.unwrap_or(false) {
            cors = cors.supports_credentials();
        }

        cors
    } else {
        Cors::permissive()
    }
}
