use crate::mq::nats::nats_config::get_nats_client;
use crate::mq::nats::nats_error::NatsError;
use crate::mq::nats::{set_nats_client, NatsConfig, NATS_CONFIG_KEY};
use async_nats::jetstream;
use async_nats::jetstream::consumer;
use async_nats::jetstream::AckKind;
use async_nats::Event;
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
use tracing::{debug, error, info, warn};
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

        // 连接生命周期事件统一打日志：库默认不输出任何事件，
        // 断连、重连、慢消费者在日志里完全不可见
        options = options.event_callback(|event| async move {
            match event {
                Event::Connected => info!("NATS 已连接（首次连接或重连成功）"),
                Event::Disconnected => warn!("NATS 连接断开，等待自动重连"),
                Event::Closed => error!("NATS 连接已关闭，不会自动重连"),
                Event::SlowConsumer(sid) => warn!("NATS 慢消费者，订阅 sid={sid} 积压"),
                Event::ServerError(e) => error!("NATS 服务端错误: {e}"),
                Event::ClientError(e) => error!("NATS 客户端错误: {e}"),
                other => debug!("NATS 事件: {other}"),
            }
        });

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
/// ## 送达语义
///
/// `persistent = false` 时返回 `Ok(())` **只表示消息已放进客户端发送缓冲**
/// （容量默认 2048），不代表服务器收到。连接断开期间调用同样返回 `Ok`，
/// 消息暂存在缓冲里等重连后发出；只有缓冲写满才会报错。
///
/// 需要确认送达请用 `persistent = true`，它会等到服务器返回 `PublishAck`。
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
/// ## ACK 语义
///
/// JetStream 模式下 ACK 由本函数负责，handler 不需要（也无法）自己确认：
/// - handler 返回 `Ok(())` → `ACK`；
/// - handler 返回 `Err` → `NAK`，服务器立即重投，重投次数上限由
///   `consumer_config.max_deliver` 控制（默认无限，业务侧务必显式设置）。
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

            // 若未设置 deliver_subject，自动生成唯一投递主题
            if consumer_config.deliver_subject.is_empty() {
                consumer_config.deliver_subject = get_nats_client()?.new_inbox();
            }

            consumer_config.filter_subject = subject.to_string();
            if let Some(ref qg) = queue_group {
                consumer_config.deliver_group = Some(qg.to_string());
            }

            let consumer_name = consumer_config
                .durable_name
                .clone()
                .or_else(|| consumer_config.name.clone())
                .unwrap_or_else(|| format!("{stream_name}-consumer"));

            let requested = consumer_config.clone();
            let consumer: consumer::Consumer<consumer::push::Config> = stream
                .get_or_create_consumer(&consumer_name, consumer_config)
                .await
                .map_err(|e| {
                    NatsError::Subscribe(format!(
                        "创建 Push Consumer {consumer_name} 于 Stream {stream_name} 失败: {e}"
                    ))
                })?;

            // get_or_create_consumer 对已存在的 consumer 原样返回，不会应用本次配置。
            // 能原地更新的字段（max_deliver / filter_subject）直接更新，避免重启才生效；
            // 不能原地更新的字段只告警，交由人决定是否重建。
            let actual_info = consumer.cached_info().clone();
            let (updatable, rebuild) = consumer_config_diffs(&requested, &actual_info.config);

            let mut max_deliver = actual_info.config.max_deliver;

            if !updatable.is_empty() {
                // 只让可更新字段取请求值，其余字段保留服务器现值，避免整次更新被拒
                let mut update = requested.clone();
                update.deliver_subject = actual_info
                    .config
                    .deliver_subject
                    .clone()
                    .unwrap_or_default();
                update.durable_name = actual_info.config.durable_name.clone();
                update.name = actual_info.config.name.clone();
                update.deliver_group = actual_info.config.deliver_group.clone();
                update.deliver_policy = actual_info.config.deliver_policy.clone();
                update.ack_policy = actual_info.config.ack_policy;

                match stream.update_consumer(update).await {
                    Ok(updated) => {
                        info!(
                            "consumer {consumer_name} 配置已原地更新: {}",
                            updatable.join("; ")
                        );
                        max_deliver = updated.cached_info().config.max_deliver;
                    }
                    Err(e) => {
                        warn!("consumer {consumer_name} 配置更新被服务器拒绝，继续用旧配置: {e}");
                    }
                }
            }

            if !rebuild.is_empty() {
                warn!(
                    "consumer {consumer_name} 存在无法原地更新的配置差异（需删除后重建）: {}",
                    rebuild.join("; ")
                );
            }

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
                            // 投递次数与原始负载要在 split 之前取，split 会消费掉 Message
                            let delivered = message.info().map(|info| info.delivered).unwrap_or(0);
                            let msg_subject = message.subject.to_string();
                            let payload = message.payload.clone();

                            // split 后仍可 ACK：Message 交给 handler，Acker 留在本层
                            let (message, acker) = message.split();
                            debug!(
                                "NATS JetStream received on {msg_subject}: {:?}",
                                String::from_utf8_lossy(&message.payload)
                            );
                            match handler(message).await {
                                Ok(()) => {
                                    if let Err(e) = acker.ack().await {
                                        error!("JetStream ACK 失败: {e}");
                                    }
                                }
                                Err(NatsError::Discard(reason)) => {
                                    error!("消息不可重试，丢弃 [{msg_subject}]: {reason}");
                                    if let Err(e) = acker.ack().await {
                                        error!("JetStream ACK 失败: {e}");
                                    }
                                }
                                Err(e) => {
                                    if max_deliver > 0 && delivered >= max_deliver {
                                        error!(
                                            "消息已达重投上限 {max_deliver}，转入死信 [{msg_subject}]: {e}"
                                        );
                                        publish_dead_letter(
                                            &msg_subject,
                                            payload,
                                            &e.to_string(),
                                            delivered,
                                        )
                                        .await;
                                        if let Err(e) = acker.ack().await {
                                            error!("JetStream ACK 失败: {e}");
                                        }
                                    } else {
                                        // NAK 让服务器立即重投，上限由 max_deliver 控制
                                        error!("处理 JetStream 消息失败，将重投 [{msg_subject}]: {e}");
                                        if let Err(e) = acker.ack_with(AckKind::Nak(None)).await {
                                            error!("JetStream NAK 失败: {e}");
                                        }
                                    }
                                }
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

/// # 比较 Consumer 配置差异
///
/// `get_or_create_consumer` 对已存在的 consumer 原样返回，**不会应用本次请求的
/// 配置**。这里把差异分成两类：
///
/// - 可原地更新：`max_deliver`、`filter_subject`，由调用方调 `update_consumer` 应用；
/// - 只能重建：`durable_name`、`deliver_group`、`deliver_policy`、`ack_policy`，
///   改动它们会破坏已有的投递绑定或消费进度，只告警。
///
/// `deliver_subject` 不参与比较：已存在的 consumer 保留首次创建时的 inbox，
/// 客户端按服务器返回的值订阅，属于预期行为。
///
/// ## 参数
///
/// - `requested`: 本次代码请求的配置。
/// - `actual`: 服务器上实际生效的配置。
///
/// ## 返回值
///
/// `(可原地更新的差异, 只能重建的差异)`，均为人类可读的描述。
fn consumer_config_diffs(
    requested: &consumer::push::Config,
    actual: &consumer::Config,
) -> (Vec<String>, Vec<String>) {
    let mut updatable = Vec::new();
    let mut rebuild = Vec::new();

    if actual.durable_name != requested.durable_name {
        rebuild.push(format!(
            "durable_name 服务器={:?} 请求={:?}",
            actual.durable_name, requested.durable_name
        ));
    }
    if actual.deliver_group != requested.deliver_group {
        rebuild.push(format!(
            "deliver_group 服务器={:?} 请求={:?}",
            actual.deliver_group, requested.deliver_group
        ));
    }
    if actual.deliver_policy != requested.deliver_policy {
        rebuild.push(format!(
            "deliver_policy 服务器={:?} 请求={:?}",
            actual.deliver_policy, requested.deliver_policy
        ));
    }
    if actual.ack_policy != requested.ack_policy {
        rebuild.push(format!(
            "ack_policy 服务器={:?} 请求={:?}",
            actual.ack_policy, requested.ack_policy
        ));
    }

    // 服务器把 0 归一化为 -1（都表示不限制），比较前先统一
    let normalize = |v: i64| if v == 0 { -1 } else { v };
    if normalize(actual.max_deliver) != normalize(requested.max_deliver) {
        updatable.push(format!(
            "max_deliver 服务器={} 请求={}",
            actual.max_deliver, requested.max_deliver
        ));
    }
    if actual.filter_subject != requested.filter_subject {
        updatable.push(format!(
            "filter_subject 服务器={:?} 请求={:?}",
            actual.filter_subject, requested.filter_subject
        ));
    }

    (updatable, rebuild)
}

/// # 死信主题前缀
///
/// 死信主题为 `dlq.{原subject}`。统一前缀是为了让消费方用一个 stream
/// （`dlq.>`）把所有死信收口——`{subject}.dlq` 这种后缀形式无法用单个通配符匹配
/// （`*` 只匹配一个 token，`>` 只能放末尾）。
pub const DLQ_SUBJECT_PREFIX: &str = "dlq.";

/// # 写入死信
///
/// 消息重投达到上限后，把原始负载转发到 `dlq.{subject}`，并把原主题、原因、
/// 投递次数放进 header，避免静默消失。
///
/// 消费方需要订阅 `dlq.>`（见 msg-svr 的 `sub::dead_letter`），否则 core NATS
/// 没有订阅者会直接丢弃。
///
/// ## 参数
///
/// - `subject`: 原消息主题。
/// - `payload`: 原始消息负载。
/// - `reason`: 转入死信的原因。
/// - `delivered`: 已经投递的次数。
async fn publish_dead_letter(subject: &str, payload: Bytes, reason: &str, delivered: i64) {
    // 死信本身处理失败时不再转发，否则会 dlq.dlq.* 无限套娃
    if subject.starts_with(DLQ_SUBJECT_PREFIX) {
        error!("死信消息处理失败，不再转发 [{subject}]: {reason}");
        return;
    }

    let client = match get_nats_client() {
        Ok(client) => client,
        Err(e) => {
            error!("写入死信失败，NATS 客户端不可用: {e}");
            return;
        }
    };

    let dlq_subject = format!("{DLQ_SUBJECT_PREFIX}{subject}");
    let delivered_text = delivered.to_string();

    let mut headers = async_nats::HeaderMap::new();
    headers.insert("Nats-Dlq-Subject", subject);
    headers.insert("Nats-Dlq-Reason", reason);
    headers.insert("Nats-Dlq-Delivered", delivered_text.as_str());

    match client
        .publish_with_headers(dlq_subject.clone(), headers, payload)
        .await
    {
        Ok(()) => info!("消息已写入死信 {dlq_subject}（原因: {reason}）"),
        Err(e) => error!("写入死信 {dlq_subject} 失败: {e}"),
    }
}