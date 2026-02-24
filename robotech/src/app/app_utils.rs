use crate::cfg::{CfgError, build_cfg};
use tracing::instrument;

#[instrument(level = "debug", err)]
pub fn build_app_cfg<'a, T: serde::Deserialize<'a>>(
    path: Option<String>,
) -> Result<(T, Vec<String>), CfgError> {
    build_cfg("APP", None, path)
}
