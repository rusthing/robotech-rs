use crate::mq::nats::nats_config::get_nats_client;
use crate::mq::nats::nats_error::NatsError;
use crate::mq::nats::{set_nats_client, NatsConfig, NATS_CONFIG_KEY};
use async_nats::jetstream;
use async_nats::jetstream::consumer;
use bytes::Bytes;
use config::Value;
use futures::StreamExt as _;
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};
use wheel_rs::config_utils::has_config_changed;

/// # 初始化 NATS 客户端连接
///
/// 根据 [`NatsConfig`] 创建或更新全局 NATS 客户端。若 `changed` 为 `None`
/// （首次加载）或配置已变更则重新连接，否则跳过以复用现有连接。
///
/// ## 参数
///
/// - `config`: NATS 连接配置，包含服务器地址、认证信息、TLS 等全部选项。
/// - `changed`: 配置变更集合，用于判断是否需要重新连接。
pub async fn setup_nats_client(
    config: NatsConfig,
    changed: &Option<HashMap<String, Value>>,
) -> Result<(), NatsError> {
    info!("setup nats client...: {config:?}");
    if changed
        .as_ref()
        .map(|changed| has_config_changed(NATS_CONFIG_KEY, changed))
        .unwrap_or(true)
    {
        let addrs = config.servers.join(",");
        let mut options = async_nats::ConnectOptions::new()
            .name(config.client_name.as_deref().unwrap_or("robotech"));

        // 默认开启初始重连；可通过设置 retry-on-initial-connect: false 关闭
        if config.retry_on_initial_connect != Some(false) {
            options = options.retry_on_initial_connect();
        }

        if let Some(token) = &config.token {
            options = options.token(token.clone());
        } else if let (Some(user), Some(password)) = (&config.user, &config.password) {
            options = options.user_and_password(user.clone(), password.clone());
        }

        if config.no_echo {
            options = options.no_echo();
        }
        if let Some(n) = config.max_reconnects {
            options = options.max_reconnects(Some(n));
        }
        if let Some(ms) = config.connection_timeout_ms {
            options = options.connection_timeout(Duration::from_millis(ms));
        }
        if config.tls_required {
            options = options.require_tls(true);
        }
        if config.tls_first {
            options = options.tls_first();
        }
        if let Some(ms) = config.ping_interval_ms {
            options = options.ping_interval(Duration::from_millis(ms));
        }
        if let Some(cap) = config.subscription_capacity {
            options = options.subscription_capacity(cap);
        }
        if let Some(cap) = config.sender_capacity {
            options = options.client_capacity(cap);
        }
        if let Some(prefix) = &config.inbox_prefix {
            options = options.custom_inbox_prefix(prefix.clone());
        }
        if let Some(ms) = config.request_timeout_ms {
            options = options.request_timeout(Some(Duration::from_millis(ms)));
        }
        if config.ignore_discovered_servers {
            options = options.ignore_discovered_servers();
        }
        if config.retain_servers_order {
            options = options.retain_servers_order();
        }
        if let Some(cap) = config.read_buffer_capacity {
            options = options.read_buffer_capacity(cap);
        }
        if config.skip_subject_validation {
            options = options.skip_subject_validation(true);
        }
        if let Some(ref addr) = config.local_address {
            let socket_addr = SocketAddr::from_str(addr)
                .map_err(|e| NatsError::Connect(format!("local-address 解析失败: {e}")))?;
            options = options.local_address(socket_addr);
        }

        // TLS 证书
        if let Some(certs) = &config.tls_root_certificates {
            for path in certs {
                options = options.add_root_certificates(PathBuf::from(path));
            }
        }
        if let (Some(cert), Some(key)) = (&config.tls_client_certificate, &config.tls_client_key) {
            options = options.add_client_certificate(PathBuf::from(cert), PathBuf::from(key));
        }

        let client = async_nats::connect_with_options(addrs, options)
            .await
            .map_err(|e| NatsError::Connect(format!("NATS 连接失败: {e}")))?;

        set_nats_client(client)
    }
    Ok(())
}

/// # 发布消息到指定 Subject
///
/// 根据 `persistent` 参数在不同模式间切换：
/// - `false`：Core NATS 发布，at-most-once 语义，无 ACK
/// - `true`：JetStream 发布，持久化到 Stream，返回确认信息
///
/// ## 参数
///
/// - `subject`: NATS 主题，如 `"orders.created"`、`"events.>"`。
/// - `payload`: 待发送的消息体，需实现 [`Serialize`]。
/// - `persistent`: `true` 走 JetStream 持久化发布，`false` 走 Core NATS。
///
/// ## 返回值
///
/// - Core NATS 模式返回 `Ok(None)`
/// - JetStream 模式返回 `Ok(Some(PublishAck))`，包含 `stream`、`sequence`、`duplicate` 等信息
pub async fn publish<T: Serialize + std::fmt::Debug>(
    subject: &str,
    payload: &T,
    persistent: bool,
) -> Result<Option<jetstream::publish::PublishAck>, NatsError> {
    let bytes = serde_json::to_vec(payload)?;
    debug!("NATS publish to {subject} (persistent={persistent}): {payload:?}");

    if persistent {
        let js = get_jetstream_context().await?;
        let ack = js
            .publish(subject.to_string(), Bytes::from(bytes))
            .await
            .map_err(|e| NatsError::Publish(format!("JetStream 发布到 {subject} 失败: {e}")))?
            .await
            .map_err(|e| NatsError::Publish(format!("JetStream 等待确认失败: {e}")))?;
        Ok(Some(ack))
    } else {
        let client = get_nats_client()?;
        client
            .publish(subject.to_string(), Bytes::from(bytes))
            .await
            .map_err(|e| NatsError::Publish(format!("发布消息到 {subject} 失败: {e}")))?;
        Ok(None)
    }
}

/// # 订阅 Subject
///
/// 根据 `stream_config` / `consumer_config` 自动选择消费模式：
/// - 均为 `None` → Core NATS 订阅，`queue_group` 可指定队列组实现负载均衡
/// - 均为 `Some` → JetStream Push 订阅，服务端推送，消费过滤由
///   `consumer_config.filter_subject` 控制
///
/// ## 参数
///
/// - `subject`: NATS 主题。
///   Core 模式直接用于订阅；JetStream 模式下 **直接覆盖**
///   `consumer_config.filter_subject`。
/// - `queue_group`: 队列组名称。
///   Core 模式用于 `queue_subscribe`；JetStream 模式下 **直接覆盖**
///   `consumer_config.deliver_group`。
/// - `stream_config`: JetStream Stream 配置，为 `Some` 时启用 JetStream 模式。
/// - `consumer_config`: JetStream Push Consumer 配置。
/// - `handler`: 消息处理函数，接收 [`async_nats::Message`]，
///   返回 `Result<(), NatsError>`。
///
/// ## 返回值
///
/// 返回 [`Arc<JoinHandle<()>>`]，可调用 `.abort()` 停止消费。
pub async fn subscribe<F, Fut>(
    subject: &str,
    queue_group: Option<&str>,
    stream_config: Option<jetstream::stream::Config>,
    consumer_config: Option<consumer::push::Config>,
    handler: F,
) -> Result<Arc<JoinHandle<()>>, NatsError>
where
    F: Fn(async_nats::Message) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), NatsError>> + Send + 'static,
{
    match (stream_config, consumer_config) {
        (None, None) => {
            let client = get_nats_client()?;
            let subj = subject.to_string();
            let mut subscriber = if let Some(queue) = queue_group {
                client
                    .queue_subscribe(subject.to_string(), queue.to_string())
                    .await
                    .map_err(|e| {
                        NatsError::Subscribe(format!("订阅 {subject}（队列组 {queue}）失败: {e}"))
                    })?
            } else {
                client
                    .subscribe(subject.to_string())
                    .await
                    .map_err(|e| NatsError::Subscribe(format!("订阅 {subject} 失败: {e}")))?
            };
            info!("NATS subscribed to subject: {subject}");

            let handle = tokio::spawn(async move {
                while let Some(message) = subscriber.next().await {
                    debug!(
                        "NATS received message on {subj}: {:?}",
                        String::from_utf8_lossy(&message.payload)
                    );
                    if let Err(e) = handler(message).await {
                        error!("处理 NATS 消息失败: {e}");
                    }
                }
                info!("NATS subscriber for {subj} stopped");
            });

            Ok(Arc::new(handle))
        }
        (Some(stream_config), Some(mut consumer_config)) => {
            let stream_name = stream_config.name.clone();
            let stream = get_or_create_stream(stream_config).await?;

            consumer_config.filter_subject = subject.to_string();
            if let Some(ref qg) = queue_group {
                consumer_config.deliver_group = Some(qg.to_string());
            }

            let consumer_name = consumer_config
                .durable_name
                .clone()
                .or_else(|| consumer_config.name.clone())
                .unwrap_or_else(|| format!("{stream_name}-consumer"));

            let consumer: consumer::Consumer<consumer::push::Config> = stream
                .get_or_create_consumer(&consumer_name, consumer_config)
                .await
                .map_err(|e| {
                    NatsError::Subscribe(format!(
                        "创建 Push Consumer {consumer_name} 于 Stream {stream_name} 失败: {e}"
                    ))
                })?;

            info!("NATS JetStream push consumer ready: {stream_name}/{consumer_name}");

            let handle = tokio::spawn(async move {
                let mut messages = match consumer.messages().await {
                    Ok(msgs) => msgs,
                    Err(e) => {
                        error!("获取 Push Consumer 消息流失败: {e}");
                        return;
                    }
                };

                while let Some(message_result) = messages.next().await {
                    match message_result {
                        Ok(message) => {
                            debug!(
                                "NATS JetStream received: {:?}",
                                String::from_utf8_lossy(&message.payload)
                            );
                            if let Err(e) = handler(message.into()).await {
                                error!("处理 JetStream 消息失败: {e}");
                            }
                        }
                        Err(e) => {
                            error!("JetStream 消息接收错误: {e}");
                        }
                    }
                }
                info!("JetStream push consumer {consumer_name} stopped");
            });

            Ok(Arc::new(handle))
        }
        _ => Err(NatsError::Subscribe(
            "stream_config 和 consumer_config 必须同时为 Some 或同时为 None".into(),
        )),
    }
}

/// # 获取 JetStream 上下文
///
/// 从当前 NATS 客户端创建 [`jetstream::Context`]，用于 Stream/Consumer 管理
/// 及 JetStream 消息发布。
///
/// ## 返回值
///
/// JetStream 上下文对象。若全局客户端未初始化则返回错误。
pub async fn get_jetstream_context() -> Result<jetstream::Context, NatsError> {
    let client = get_nats_client()?;
    let js = jetstream::new(client);
    Ok(js)
}

/// # 创建或获取 JetStream Stream
///
/// 若指定名称的 Stream 不存在则按 `stream_config` 创建，存在则返回已有的
/// Stream 句柄。
///
/// ## 参数
///
/// - `stream_config`: Stream 配置，需提供 `name` 及 `subjects` 等完整配置。
///
/// ## 返回值
///
/// JetStream Stream 句柄，可通过它进一步创建 Consumer。
pub async fn get_or_create_stream(
    stream_config: jetstream::stream::Config,
) -> Result<jetstream::stream::Stream, NatsError> {
    let js = get_jetstream_context().await?;
    let stream_name = stream_config.name.clone();

    let stream = js
        .get_or_create_stream(stream_config)
        .await
        .map_err(|e| NatsError::Connect(format!("创建/获取 Stream {stream_name} 失败: {e}")))?;

    info!("NATS JetStream stream ready: {stream_name}");
    Ok(stream)
}