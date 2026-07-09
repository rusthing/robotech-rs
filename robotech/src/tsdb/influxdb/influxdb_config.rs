use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct InfluxdbConfig {
    /// 数据库URL
    #[serde()]
    pub url: String,
    /// 桶(数据库)
    #[serde()]
    pub bucket: String,
    /// 量度(表)
    #[serde()]
    pub measurement: Option<String>,
    /// 数据库token
    #[serde()]
    pub token: String,
    /// 连接池最大大小
    #[serde(default = "default_pool_max_size")]
    pub pool_max_size: usize,
}

fn default_pool_max_size() -> usize {
    5000
}
