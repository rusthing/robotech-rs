use crate::tsdb::influxdb::influxdb_error::InfluxdbError;
use crate::tsdb::influxdb::InfluxdbConfig;
use influxdb::Client;
use reqwest::ClientBuilder;

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
