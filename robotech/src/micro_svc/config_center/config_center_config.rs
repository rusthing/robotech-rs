use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use wheel_rs::serde::{path_buf_serde, vec_serde};

/// 配置中心的本地配置：配置文件格式、公共配置列表与本地快照目录。
///
/// `common_configs` 是需要加载的公共配置项列表；`snapshot_dir` 为本地快照目录，
/// 当后端拉取失败时用于回退到最近一次成功获取的配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigCenterConfig {
    /// 配置文件格式
    pub file_format: String,
    /// 公共配置
    #[serde(with = "vec_serde", default)]
    pub common_configs: Vec<String>,
    /// 本地快照目录
    #[serde(with = "path_buf_serde", default = "snapshot_dir_default")]
    pub snapshot_dir: PathBuf,
}

fn snapshot_dir_default() -> PathBuf {
    PathBuf::from("snapshot")
}