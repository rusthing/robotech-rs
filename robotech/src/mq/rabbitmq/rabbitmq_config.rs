use crate::mq::rabbitmq::rabbitmq_error::RabbitMqError;
use arc_swap::ArcSwapOption;
use lapin::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// RabbitMQ 配置键
///
/// 用于标识配置文件中的 RabbitMQ 配置段，及配置变更检测。
pub const RABBITMQ_CONFIG_KEY: &str = "rabbitmq";

/// 全局 RabbitMQ 连接
///
/// 通过 [`ArcSwapOption`] 实现无锁的运行时替换，配置热更新时无需重启即可切换连接。
static RABBITMQ_CONNECTION: ArcSwapOption<Arc<Connection>> = ArcSwapOption::const_empty();

/// 获取全局 RabbitMQ 连接
///
/// 从全局 [`RABBITMQ_CONNECTION`] 中加载当前连接，若尚未初始化则返回错误。
pub fn get_rabbitmq_connection() -> Result<Arc<Connection>, RabbitMqError> {
    RABBITMQ_CONNECTION
        .load_full()
        .map(|outer| outer.as_ref().clone())
        .ok_or(RabbitMqError::Connect("RabbitMQ 客户端未初始化".to_string()))
}

/// 设置全局 RabbitMQ 连接
///
/// 将新的 RabbitMQ 连接存入全局变量，替换旧的连接。
pub fn set_rabbitmq_connection(connection: Connection) {
    RABBITMQ_CONNECTION.store(Some(Arc::new(Arc::new(connection))));
}

/// RabbitMQ 连接配置
///
/// 对应 RabbitMQ 客户端的全部可配置属性。
/// 支持通过 AMQP URI 或独立字段两种方式配置连接参数。
///
/// ## 配置示例
///
/// ```yaml
/// rabbitmq:
///   uri: amqp://guest:guest@localhost:5672/%2f
///   heartbeat: 60
///   connection-timeout-ms: 10000
/// ```
///
/// 或使用独立字段：
///
/// ```yaml
/// rabbitmq:
///   host: localhost
///   port: 5672
///   username: guest
///   password: guest
///   vhost: /
/// ```
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct RabbitMqConfig {
    /// AMQP 连接 URI
    ///
    /// 格式: `amqp://user:pass@host:port/vhost`
    /// 当设置了 `uri` 时，会优先使用 URI 连接，忽略 `host`、`port`、`username`、
    /// `password`、`vhost` 等独立字段。
    pub uri: Option<String>,

    /// RabbitMQ 服务器地址
    ///
    /// 默认值 `"localhost"`。
    pub host: Option<String>,

    /// RabbitMQ 服务器端口
    ///
    /// 默认值 `5672`。
    pub port: Option<u16>,

    /// 用户名
    ///
    /// 默认值 `"guest"`。
    pub username: Option<String>,

    /// 密码
    ///
    /// 默认值 `"guest"`。
    pub password: Option<String>,

    /// 虚拟主机
    ///
    /// 默认值 `"/"`。
    pub vhost: Option<String>,

    /// 心跳间隔，单位秒
    ///
    /// 默认值 `60`，设为 `0` 表示禁用心跳。
    pub heartbeat: Option<u16>,

    /// 连接超时，单位毫秒
    ///
    /// 默认值 `10000`。
    pub connection_timeout_ms: Option<u64>,

    /// 自动重连
    ///
    /// 启用后连接断开时自动重连，默认 `true`。
    pub auto_reconnect: Option<bool>,

    /// 最大重连次数
    ///
    /// `None` 表示无限制重连，默认 `None`。
    pub max_reconnects: Option<usize>,

    /// 重连间隔，单位毫秒
    ///
    /// 默认值 `3000`。
    pub reconnect_interval_ms: Option<u64>,
}

impl RabbitMqConfig {
    /// 构建 AMQP 连接 URI
    ///
    /// 如果配置了 `uri` 则直接返回；否则根据独立字段拼接 URI。
    /// heartbeat 通过 URI query 参数传递。
    pub fn build_uri(&self) -> String {
        let base_uri = if let Some(ref uri) = self.uri {
            uri.clone()
        } else {
            let host = self.host.as_deref().unwrap_or("localhost");
            let port = self.port.unwrap_or(5672);
            let username = self.username.as_deref().unwrap_or("guest");
            let password = self.password.as_deref().unwrap_or("guest");
            let vhost = self.vhost.as_deref().unwrap_or("/");
            let encoded_vhost = if vhost == "/" {
                "%2f".to_string()
            } else {
                vhost.to_string()
            };
            format!("amqp://{username}:{password}@{host}:{port}/{encoded_vhost}")
        };

        if let Some(heartbeat) = self.heartbeat {
            if heartbeat > 0 && !base_uri.contains("heartbeat=") {
                let separator = if base_uri.contains('?') { "&" } else { "?" };
                return format!("{base_uri}{separator}heartbeat={heartbeat}");
            }
        }
        base_uri
    }
}