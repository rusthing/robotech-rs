use crate::mq::rabbitmq::rabbitmq_config::get_rabbitmq_connection;
use crate::mq::rabbitmq::rabbitmq_error::RabbitMqError;
use crate::mq::rabbitmq::{set_rabbitmq_connection, RabbitMqConfig, RABBITMQ_CONFIG_KEY};
use config::Value;
use futures::StreamExt as _;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
    BasicQosOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind};
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use wheel_rs::config_utils::has_config_changed;

/// 初始化 RabbitMQ 客户端连接
///
/// 根据 [`RabbitMqConfig`] 创建或更新全局 RabbitMQ 连接。若 `changed` 为 `None`
/// （首次加载）或配置已变更则重新连接，否则跳过以复用现有连接。
///
/// ## 参数
///
/// - `config`: RabbitMQ 连接配置。
/// - `changed`: 配置变更集合，用于判断是否需要重新连接。
pub async fn setup_rabbitmq_client(
    config: RabbitMqConfig,
    changed: &Option<HashMap<String, Value>>,
) -> Result<(), RabbitMqError> {
    info!("setup rabbitmq client...: {config:?}");
    if changed
        .as_ref()
        .map(|changed| has_config_changed(RABBITMQ_CONFIG_KEY, changed))
        .unwrap_or(true)
    {
        let uri = config.build_uri();

        let mut props = ConnectionProperties::default().with_connection_name("robotech".into());

        if config.auto_reconnect.unwrap_or(true) {
            props = props.enable_auto_recover();
        }

        // set max retry attempts via backoff configuration
        if let Some(max_retries) = config.max_reconnects {
            if max_retries > 0 {
                props = props
                    .configure_backoff(|b| b.with_max_times(max_retries));
            }
        }

        let connection = Connection::connect(&uri, props)
            .await
            .map_err(|e| RabbitMqError::Connect(format!("RabbitMQ 连接失败: {e}")))?;

        info!("RabbitMQ 连接成功: {uri}");
        set_rabbitmq_connection(connection);
    }
    Ok(())
}

/// 创建 Channel
///
/// 从全局连接中创建新的 Channel，并设置 QoS prefetch count。
///
/// ## 参数
///
/// - `prefetch_count`: 消费者预取数量，控制未确认消息的最大数量。默认 `10`。
pub async fn create_channel(prefetch_count: Option<u16>) -> Result<Channel, RabbitMqError> {
    let connection = get_rabbitmq_connection()?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|e| RabbitMqError::Channel(format!("创建 Channel 失败: {e}")))?;

    let count = prefetch_count.unwrap_or(10);
    channel
        .basic_qos(count, BasicQosOptions::default())
        .await
        .map_err(|e| RabbitMqError::Channel(format!("设置 QoS 失败: {e}")))?;

    Ok(channel)
}

/// 声明 Exchange
///
/// 在指定 Channel 上声明 Exchange，若已存在则跳过（幂等）。
///
/// ## 参数
///
/// - `channel`: RabbitMQ Channel。
/// - `exchange`: Exchange 名称。
/// - `kind`: Exchange 类型（direct、fanout、topic、headers）。
/// - `durable`: 是否持久化，默认 `true`。
/// - `auto_delete`: 是否自动删除，默认 `false`。
pub async fn declare_exchange(
    channel: &Channel,
    exchange: &str,
    kind: ExchangeKind,
    durable: Option<bool>,
    auto_delete: Option<bool>,
) -> Result<(), RabbitMqError> {
    let options = ExchangeDeclareOptions {
        durable: durable.unwrap_or(true),
        auto_delete: auto_delete.unwrap_or(false),
        ..ExchangeDeclareOptions::default()
    };
    channel
        .exchange_declare(exchange.into(), kind, options, FieldTable::default())
        .await
        .map_err(|e| RabbitMqError::Channel(format!("声明 Exchange {exchange} 失败: {e}")))?;
    debug!("Exchange 声明成功: {exchange}");
    Ok(())
}

/// 声明 Queue
///
/// 在指定 Channel 上声明 Queue，若已存在则跳过（幂等）。
///
/// ## 参数
///
/// - `channel`: RabbitMQ Channel。
/// - `queue`: Queue 名称。
/// - `durable`: 是否持久化，默认 `true`。
/// - `auto_delete`: 是否自动删除，默认 `false`。
/// - `exclusive`: 是否独占，默认 `false`。
pub async fn declare_queue(
    channel: &Channel,
    queue: &str,
    durable: Option<bool>,
    auto_delete: Option<bool>,
    exclusive: Option<bool>,
) -> Result<(), RabbitMqError> {
    let options = QueueDeclareOptions {
        durable: durable.unwrap_or(true),
        auto_delete: auto_delete.unwrap_or(false),
        exclusive: exclusive.unwrap_or(false),
        ..QueueDeclareOptions::default()
    };
    channel
        .queue_declare(queue.into(), options, FieldTable::default())
        .await
        .map_err(|e| RabbitMqError::Channel(format!("声明 Queue {queue} 失败: {e}")))?;
    debug!("Queue 声明成功: {queue}");
    Ok(())
}

/// 绑定 Queue 到 Exchange
///
/// ## 参数
///
/// - `channel`: RabbitMQ Channel。
/// - `queue`: Queue 名称。
/// - `exchange`: Exchange 名称。
/// - `routing_key`: 路由键。
pub async fn bind_queue(
    channel: &Channel,
    queue: &str,
    exchange: &str,
    routing_key: &str,
) -> Result<(), RabbitMqError> {
    channel
        .queue_bind(
            queue.into(),
            exchange.into(),
            routing_key.into(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|e| RabbitMqError::Channel(format!("绑定 Queue {queue} 到 Exchange {exchange} 失败: {e}")))?;
    debug!("Queue {queue} 绑定到 Exchange {exchange} (routing_key: {routing_key})");
    Ok(())
}

/// 发布消息到 Exchange
///
/// 将消息序列化为 JSON 后发布到指定 Exchange。
///
/// ## 参数
///
/// - `exchange`: Exchange 名称。
/// - `routing_key`: 路由键。
/// - `payload`: 待发送的消息体，需实现 [`Serialize`]。
/// - `mandatory`: 是否强制路由（无法路由时返回消息），默认 `false`。
/// - `persistent`: 是否持久化消息（delivery_mode=2），默认 `true`。
pub async fn publish<T: Serialize + std::fmt::Debug>(
    exchange: &str,
    routing_key: &str,
    payload: &T,
    mandatory: Option<bool>,
    persistent: Option<bool>,
) -> Result<(), RabbitMqError> {
    let channel = create_channel(None).await?;
    let bytes = serde_json::to_vec(payload)?;
    debug!("RabbitMQ publish to {exchange} (routing_key={routing_key}): {payload:?}");

    let options = BasicPublishOptions {
        mandatory: mandatory.unwrap_or(false),
        ..BasicPublishOptions::default()
    };

    let mut props = BasicProperties::default();
    if persistent.unwrap_or(true) {
        props = props.with_delivery_mode(2);
    }
    props = props.with_content_type("application/json".into());

    channel
        .basic_publish(
            exchange.into(),
            routing_key.into(),
            options,
            &bytes,
            props,
        )
        .await
        .map_err(|e| RabbitMqError::Publish(format!("发布消息到 {exchange} 失败: {e}")))?;

    Ok(())
}

/// 消费队列消息
///
/// 启动一个异步任务持续消费指定队列的消息，每条消息交由 `handler` 处理。
/// 处理成功后自动 ACK，处理失败则 NACK 并重新入队。
///
/// ## 参数
///
/// - `queue`: Queue 名称。
/// - `consumer_tag`: 消费者标签，用于标识消费者。
/// - `prefetch_count`: 消费者预取数量，默认 `10`。
/// - `auto_ack`: 是否自动 ACK（默认为 `false`，手动 ACK）。
/// - `handler`: 消息处理函数，接收消息体字节数组，返回 `Result<(), RabbitMqError>`。
///
/// ## 返回值
///
/// 返回 [`Arc<JoinHandle<()>>`]，可调用 `.abort()` 停止消费。
pub async fn consume<F, Fut>(
    queue: &str,
    consumer_tag: &str,
    prefetch_count: Option<u16>,
    auto_ack: Option<bool>,
    handler: F,
) -> Result<Arc<JoinHandle<()>>, RabbitMqError>
where
    F: Fn(Vec<u8>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), RabbitMqError>> + Send + 'static,
{
    let channel = create_channel(prefetch_count).await?;
    let auto_ack = auto_ack.unwrap_or(false);
    let consumer_tag: lapin::types::ShortString = consumer_tag.into();
    let queue: lapin::types::ShortString = queue.into();
    let queue_str = queue.to_string();

    let handle = tokio::spawn(async move {
        let mut consumer = match channel
            .basic_consume(
                queue.clone(),
                consumer_tag.clone(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
        {
            Ok(consumer) => consumer,
            Err(e) => {
                error!("启动消费者失败: {e}");
                return;
            }
        };

        info!("RabbitMQ 消费者启动成功: queue={queue_str}, tag={consumer_tag}");

        while let Some(delivery_result) = consumer.next().await {
            match delivery_result {
                Ok(delivery) => {
                    debug!(
                        "收到 RabbitMQ 消息: delivery_tag={}, exchange={}, routing_key={}",
                        delivery.delivery_tag,
                        delivery.exchange,
                        delivery.routing_key
                    );

                    match handler(delivery.data.clone()).await {
                        Ok(()) => {
                            if !auto_ack {
                                if let Err(e) = delivery
                                    .acker
                                    .ack(BasicAckOptions::default())
                                    .await
                                {
                                    error!("ACK 消息失败: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            error!("处理 RabbitMQ 消息失败: {e}");
                            if !auto_ack {
                                if let Err(e) = delivery
                                    .acker
                                    .nack(BasicNackOptions {
                                        requeue: true,
                                        ..BasicNackOptions::default()
                                    })
                                    .await
                                {
                                    error!("NACK 消息失败: {e}");
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("RabbitMQ 消费消息出错: {e}");
                }
            }
        }
        warn!("RabbitMQ 消费者退出: queue={queue_str}");
    });

    Ok(Arc::new(handle))
}