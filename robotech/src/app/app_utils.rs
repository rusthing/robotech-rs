use crate::cfg::{build_cfg, CfgError};
use robotech_macros::log_call;

#[log_call]
pub fn build_app_cfg<'a, T: serde::Deserialize<'a> + std::fmt::Debug>(
    path: Option<String>,
) -> Result<(T, Vec<String>), CfgError> {
    build_cfg("APP", None, path)
}
