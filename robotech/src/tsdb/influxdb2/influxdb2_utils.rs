use crate::tsdb::influxdb2::Influxdb2Config;
use influxdb2::Client;

pub fn build_influxdb2_client(config: Influxdb2Config) -> Client {
    let Influxdb2Config {
        url, org, token, ..
    } = config;
    Client::new(url, org, token)
}
