use crate::cfg::{CfgError, build_config};
use tracing::instrument;

#[instrument(level = "debug", err)]
pub fn build_app_config<'a, T: serde::Deserialize<'a>>(
    path: Option<String>,
) -> Result<(T, Vec<String>), CfgError> {
    build_config("APP", None, path)
}
