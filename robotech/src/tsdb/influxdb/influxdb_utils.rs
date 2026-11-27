use crate::tsdb::influxdb::influxdb_error::InfluxdbError;
use crate::tsdb::influxdb::InfluxdbConfig;
use reqwest::ClientBuilder;
use reqwest::header::AUTHORIZATION;

/// InfluxDB 2.x 写入客户端。
///
/// 写入走原生 2.x 接口 `POST /api/v2/write?org=<org>&bucket=<bucket>&precision=ms`，
/// 不再依赖 1.x 兼容接口（`/write?db=`，需要库侧 DBRP 映射）。
#[derive(Debug, Clone)]
pub struct InfluxdbWriter {
    /// 底层 HTTP 客户端（带连接池上限）
    http: reqwest::Client,
    /// 服务地址，已去掉末尾 `/`
    base_url: String,
    /// 组织名
    org: String,
    /// 桶名
    bucket: String,
    /// 访问 token
    token: String,
}

impl InfluxdbWriter {
    /// 批量写入 line protocol（多行用 `\n` 拼好）。
    pub async fn write_lines(&self, lines: String) -> Result<(), InfluxdbError> {
        let url = format!("{}/api/v2/write", self.base_url);
        let res = self
            .http
            .post(url)
            .query(&[
                ("org", self.org.as_str()),
                ("bucket", self.bucket.as_str()),
                ("precision", "ms"),
            ])
            .header(AUTHORIZATION, format!("Token {}", self.token))
            .body(lines)
            .send()
            .await
            .map_err(|e| InfluxdbError::Request(e.to_string()))?;

        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(InfluxdbError::Api {
                status: status.as_u16(),
                body,
            });
        }
        Ok(())
    }
}

/// 根据配置构建 InfluxDB 2.x 写入客户端。
///
/// 使用带连接池上限（`pool_max_size`）的 reqwest 客户端承载底层 HTTP 请求。
///
/// ## 错误
/// reqwest 客户端构建失败时返回 `InfluxdbError::Build`。
pub fn build_influxdb_client(config: InfluxdbConfig) -> Result<InfluxdbWriter, InfluxdbError> {
    let InfluxdbConfig {
        url,
        org,
        bucket,
        token,
        pool_max_size,
        ..
    } = config;
    let http = ClientBuilder::new()
        .pool_max_idle_per_host(pool_max_size)
        .build()?;
    Ok(InfluxdbWriter {
        http,
        base_url: url.trim_end_matches('/').to_string(),
        org,
        bucket,
        token,
    })
}
