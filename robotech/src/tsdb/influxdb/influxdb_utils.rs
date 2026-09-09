use crate::tsdb::influxdb::influxdb_error::InfluxdbError;
use crate::tsdb::influxdb::InfluxdbConfig;
use influxdb::Client;
use reqwest::ClientBuilder;

/// 根据配置构建 InfluxDB 客户端。
///
/// 使用带连接池上限（`pool_max_size`）的 reqwest 客户端承载底层 HTTP 请求。
///
/// ## 错误
/// reqwest 客户端构建失败时返回 `InfluxdbError::Build`。
pub fn build_influxdb_client(config: InfluxdbConfig) -> Result<Client, InfluxdbError> {
    let InfluxdbConfig {
        url,
        bucket,
        token,
        pool_max_size,
        ..
    } = config;
    let mut client = Client::new(url, bucket);
    client = client.with_token(token);
    client = client.with_http_client(
        ClientBuilder::new()
            .pool_max_idle_per_host(pool_max_size)
            .build()?,
    );
    Ok(client)
}
