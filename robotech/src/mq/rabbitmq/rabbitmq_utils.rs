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
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::Semaphore;
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
    info!("setup RabbitMQ client...: {config:?}");
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
/// - `arguments`: 队列额外参数（如 `x-message-ttl`、`x-max-length` 等）。
pub async fn declare_queue(
    channel: &Channel,
    queue: &str,
    durable: Option<bool>,
    auto_delete: Option<bool>,
    exclusive: Option<bool>,
    arguments: Option<FieldTable>,
) -> Result<(), RabbitMqError> {
    let options = QueueDeclareOptions {
        durable: durable.unwrap_or(true),
        auto_delete: auto_delete.unwrap_or(false),
        exclusive: exclusive.unwrap_or(false),
        ..QueueDeclareOptions::default()
    };
    channel
        .queue_declare(queue.into(), options, arguments.unwrap_or_default())
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

/// 消费队列消息（串行）
///
/// 等价于 [`subscribe_with_concurrency`] 的 `max_concurrency = 1`。需要「一条
/// 卡住不拖累整条队列」时用那个带并发度的版本。
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
pub async fn subscribe<F, Fut>(
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
    subscribe_with_concurrency(queue, consumer_tag, prefetch_count, auto_ack, 1, handler).await
}

/// # 消费队列消息（带并发度）
///
/// ## 为什么需要这个
///
/// `prefetch_count` **不带来并发**：它只决定 broker 一次往客户端推多少条，
/// 而消费循环是「处理完一条才取下一条」，并发度恒为 1。prefetch 的唯一效果是
/// 把消息提前堆进客户端内存——出问题时它们既不在 broker 里（不能被重新分配），
/// 也不在处理（因为前一条卡住了）。
///
/// 这里用信号量限制**在途**消息数，每条消息 spawn 一个 task 处理、在 task 内
/// ACK/NACK。`prefetch` 建议配成 `max_concurrency * 2`：够填满并发，又不会在
/// 进程崩溃时让太多消息悬在半空。
///
/// ## 顺序
///
/// 并发处理意味着**同一队列内的消息顺序不再保证**，业务上要求严格顺序的队列
/// 用 [`subscribe`]。
///
/// ## 失败处理
///
/// - 成功 → ACK；
/// - [`RabbitMqError::Discard`]（永久失败）→ ACK 丢弃，不让毒消息继续占着队列；
/// - 其它 → 按**连续失败次数**指数退避后 NACK 重投。失败计数跨 task 共享：
///   下游整体挂掉时所有 task 一起失败，退避该按整体算，而不是每条各算各的。
pub async fn subscribe_with_concurrency<F, Fut>(
    queue: &str,
    consumer_tag: &str,
    prefetch_count: Option<u16>,
    auto_ack: Option<bool>,
    max_concurrency: usize,
    handler: F,
) -> Result<Arc<JoinHandle<()>>, RabbitMqError>
where
    F: Fn(Vec<u8>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), RabbitMqError>> + Send + 'static,
{
    let channel = create_channel(prefetch_count).await?;
    let auto_ack = auto_ack.unwrap_or(false);
    let concurrency = max_concurrency.max(1);
    let consumer_tag: lapin::types::ShortString = consumer_tag.into();
    let queue: lapin::types::ShortString = queue.into();
    let queue_str = queue.to_string();

    let handler = Arc::new(handler);
    let permits = Arc::new(Semaphore::new(concurrency));
    let consecutive_failures = Arc::new(AtomicU32::new(0));

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

        info!(
            "RabbitMQ 消费者启动成功: queue={queue_str}, tag={consumer_tag}, concurrency={concurrency}"
        );

        while let Some(delivery_result) = consumer.next().await {
            match delivery_result {
                Ok(delivery) => {
                    // 拿不到许可就在这里等：此时不再从 broker 取新消息，
                    // 在途数量被卡在 concurrency 条以内
                    let permit = match permits.clone().acquire_owned().await {
                        Ok(permit) => permit,
                        Err(_) => break,
                    };

                    let handler = Arc::clone(&handler);
                    let failures = Arc::clone(&consecutive_failures);
                    let queue_name = queue_str.clone();

                    tokio::spawn(async move {
                        // 许可在 task 结束时才释放，正好覆盖整段处理时间
                        let _permit = permit;
                        handle_delivery(delivery, &*handler, auto_ack, &failures, &queue_name).await;
                    });
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

/// # 处理一条消息并确认
///
/// 抽出来是为了让 [`subscribe_with_concurrency`] 的循环体只负责分发。
async fn handle_delivery<F, Fut>(
    delivery: lapin::message::Delivery,
    handler: &F,
    auto_ack: bool,
    consecutive_failures: &AtomicU32,
    queue: &str,
) where
    F: Fn(Vec<u8>) -> Fut,
    Fut: Future<Output = Result<(), RabbitMqError>>,
{
    debug!(
        "收到 RabbitMQ 消息: delivery_tag={}, exchange={}, routing_key={}",
        delivery.delivery_tag, delivery.exchange, delivery.routing_key
    );

    match handler(delivery.data.clone()).await {
        Ok(()) => {
            consecutive_failures.store(0, Ordering::Relaxed);
            if !auto_ack {
                if let Err(e) = delivery.acker.ack(BasicAckOptions::default()).await {
                    error!("ACK 消息失败: {e}");
                }
            }
        }
        // 永久失败：ack 丢弃。这是「一条毒消息堵死整条队列」的唯一解：
        // 消息本身报不上去，重投不会改变结果，而 requeue 的位置就在队头。
        Err(e) if e.is_permanent() => {
            error!("处理 RabbitMQ 消息永久失败，已确认丢弃: queue={queue}, {e}");
            if !auto_ack {
                if let Err(e) = delivery.acker.ack(BasicAckOptions::default()).await {
                    error!("ACK 消息失败: {e}");
                }
            }
        }
        Err(e) => {
            let failures = consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
            let delay = retry_backoff(failures);
            error!(
                "处理 RabbitMQ 消息失败（连续第 {failures} 次），{delay:?} 后重投: queue={queue}, {e}"
            );
            if !auto_ack {
                tokio::time::sleep(delay).await;
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

/// # 连续失败后的重投退避时长
///
/// 100ms 起、每次翻倍、封顶 5s：偶发抖动很快恢复，持续故障也不会把 CPU 和
/// 日志打满。
///
/// 退避是**整个消费者**的，不是单条消息的：一次失败期间不取新消息。对单队列
/// 单消费者来说这正是想要的效果——下游已经不可用，此刻去取更多消息也只是
/// 排着队一起失败。
fn retry_backoff(consecutive_failures: u32) -> std::time::Duration {
    const BASE_MS: u64 = 100;
    const MAX_MS: u64 = 5_000;

    // 移位上限 6（100ms << 6 = 6.4s，被 MAX_MS 截到 5s），避免移位溢出
    let shift = consecutive_failures.saturating_sub(1).min(6);
    std::time::Duration::from_millis((BASE_MS << shift).min(MAX_MS))
}