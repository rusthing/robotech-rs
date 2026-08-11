use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use wheel_rs::serde::path_buf_serde;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigCenterConfig {
    /// 快照目录
    #[serde(with = "path_buf_serde", default = "snapshot_dir_default")]
    pub snapshot_dir: PathBuf,
}

fn snapshot_dir_default() -> PathBuf {
    PathBuf::from("snapshot")
}
