use crate::redis::get_redis_conn;
use redis::streams::{
    StreamAddOptions, StreamAutoClaimOptions, StreamAutoClaimReply, StreamInfoConsumer,
    StreamInfoConsumersReply, StreamReadOptions, StreamReadReply, StreamTrimOptions,
    StreamTrimStrategy, StreamTrimmingMode,
};
use redis::{AsyncCommands, RedisError};
use std::time::Duration;

/// # 消息发布到 Stream
/// 执行 XADD 命令将一组键值对作为消息添加到指定流中。
/// - stream_key: 流的键名。
/// - fields: 消息的字段键值对切片，格式为 &[(key, value)]。
/// 返回新消息的 ID，格式为 "{毫秒时间戳}-{序号}"。
pub async fn publish_to_stream(
    stream_key: &str,
    fields: &[(&str, &str)],
) -> Result<String, RedisError> {
    let mut conn = get_redis_conn()?;
    conn.xadd(stream_key, "*", fields).await
}

/// # 消息发布到 Stream（写入时顺带裁剪上限）
///
/// 与 `publish_to_stream` 相同，额外通过 `XADD <key> MAXLEN ~ <max_len> * ...`
/// 在写入的同时裁剪流，使长度长期保持在 `max_len` 附近，避免无限增长。
///
/// 采用 `~`（近似）模式（[`StreamTrimmingMode::Approx`]）：Redis 只在整块（macro node）
/// 粒度上裁剪，不会为精确计数而搬移内部结构，开销远低于精确裁剪；代价是实际长度
/// 可能略高于 `max_len`。
///
/// 注意：裁剪删的是**最早**的条目，不区分是否已被消费。若消费者长时间落后，
/// 未消费的消息可能被裁掉 —— 本项目里调度器会按 `NOT EXISTS(jobx_task...)` 幂等重发，
/// 因此可以自愈。
///
/// - stream_key: 流的键名。
/// - fields: 消息的字段键值对切片，格式为 &[(key, value)]。
/// - max_len: 保留的最大消息条数（近似值）。
/// 返回 `Option<String>`：`Some(消息ID)` 表示发布成功（未使用 NOMKSTREAM，正常不会为 None）。
pub async fn publish_to_stream_with_maxlen(
    stream_key: &str,
    fields: &[(&str, &str)],
    max_len: usize,
) -> Result<Option<String>, RedisError> {
    let mut conn = get_redis_conn()?;
    let opts = StreamAddOptions::default().trim(StreamTrimStrategy::maxlen(
        StreamTrimmingMode::Approx,
        max_len,
    ));
    conn.xadd_options(stream_key, "*", fields, &opts).await
}

/// # 仅在 Stream 为空时发布消息
///
/// ⚠️ **长期运行的 Stream 不要用这个函数。** 它是"空才发"的单槽位语义：
/// 只有 `XLEN == 0` 时才 `XADD`，否则返回 `None`。而 `XACK` 只把消息从消费者 PEL 移除，
/// **不会删除 Stream 里的条目**；若没有任何地方调用 `XDEL`/`XTRIM`，
/// 第一次 `XADD` 之后 `XLEN` 就永远 ≥ 1 —— 该 Stream 在**整个生命周期内只能承载一条消息**，
/// 后续所有发布都会被静默拒绝。需要去重请用业务侧幂等（如唯一键 / NOT EXISTS 子查询），
/// 需要控制长度请用 [`publish_to_stream_with_maxlen`]。
///
/// 使用 Lua 脚本原子性地检查 Stream 是否为空（XLEN == 0）：若为空则发布消息，
/// 若已有消息则不做任何操作。相比 EXISTS，XLEN 能正确处理消息被 XDEL 删除后
/// Stream key 仍存在的场景。
/// - stream_key: 流的键名。
/// - fields: 消息的字段键值对切片，格式为 &[(key, value)]。
/// 返回 Option<String>：Some(消息ID) 表示发布成功，None 表示 Stream 中已有消息、未执行任何操作。
pub async fn publish_to_stream_if_not_exists(
    stream_key: &str,
    fields: &[(&str, &str)],
) -> Result<Option<String>, RedisError> {
    let mut conn = get_redis_conn()?;

    let script = redis::Script::new(
        r#"if redis.call('XLEN', KEYS[1]) == 0 then return redis.call('XADD', KEYS[1], '*', unpack(ARGV)) else return nil end"#,
    );
    let mut invocation = script.key(stream_key);
    for (k, v) in fields {
        invocation.arg(*k);
        invocation.arg(*v);
    }

    invocation.invoke_async(&mut conn).await
}

/// # 统一读取流消息
///
/// 同时支持普通读取（XREAD）和消费者组读取（XREADGROUP），根据 `group` 参数自动选择。
/// 当 `block_duration` 为 `Some` 时阻塞等待新消息，可避免空轮询浪费 CPU 与网络；
/// 为 `None` 时非阻塞立即返回。
///
/// # ⚠️ 使用 `block_duration` 前必读
///
/// redis-rs 从 1.0 起给 async 连接加了默认 `response_timeout = 500ms`
/// （`redis-1.7.1/src/client.rs` 的 `DEFAULT_RESPONSE_TIMEOUT`；
/// `Client::get_multiplexed_async_connection()` 内部即 `AsyncConnectionConfig::new()`）。
/// **只要 `block_duration` 超过这个值，读取就会在 500ms 被客户端掐断并返回
/// `Err(timed out)`**。更麻烦的是客户端放弃后**不会取消服务端已发出的 XREADGROUP** ——
/// 消息到达时服务端照常投递并写进消费者 PEL，回复却被丢弃，消息就此**永久滞留 PEL**
/// （`">"` 不会再返回它），而且这类失败只体现为超时，很容易被静默吞掉。
///
/// 因此要用阻塞读，必须同时满足两条：
/// 1. 给它一条**独占**连接。本函数默认走 `get_redis_conn()` 返回的**进程内全局共享**
///    MultiplexedConnection，阻塞期间会占住整条连接，破坏多路复用的前提；
/// 2. 把该连接的 `response_timeout` 调大或关掉，例如
///    `AsyncConnectionConfig::new().set_response_timeout(None)`。
///
/// 更省事的做法是干脆不阻塞：`block_duration` 传 `None`，靠调用方自己的轮询间隔控制节奏。
///
/// ## `last_id` 参数的含义取决于是否有消费者组
///
/// ### 无消费者组（`group = None`，走 XREAD）
/// `last_id` 就是一个普通的消息 ID，读取所有 ID 大于它的消息：
/// - `"0"` — 从 Stream 的第一条消息开始读取（读所有历史消息）
/// - `"$"` — 只读取调用之后新增的消息（跳过历史）
/// - `"1234567890-0"` — 从指定消息之后开始读
///
/// ### 有消费者组（`group = Some(...)`，走 XREADGROUP）
/// `last_id` 的含义完全不同，**只跟当前消费者有关**：
/// - `">"` — 读取**从未投递给组内任何消费者**的新消息。每次调用还会把返回的消息
///   记入当前消费者的 PEL（Pending Entries List），等待后续 ACK。常规消费就用这个。
/// - `"0"` — 读取**已投递给当前消费者、但尚未 ACK** 的消息（即回放当前消费者自己的 PEL）。
///   注意：`"0"` 不会返回从未投递过的新消息，也不会返回其他消费者的 pending 消息。
///   只有当同一个 consumer name 重启后，用它来回放自己之前没来得及 ACK 的消息才有意义；
///   如果想认领其他消费者超时未 ACK 的消息，需要用 XAUTOCLAIM。
/// - 具体消息 ID — 回放当前消费者 PEL 中 ID 大于该值的 pending 消息（通常配合上次已处理的最大 ID 做断点续传）。
///
/// 简单说：`">"` = 收新消息，`"0"` = 回放自己还没 ACK 的消息。
///
/// - stream_key: 流的键名。
/// - last_id: 起始消息 ID，含义见上。
/// - count: 单次最多读取的消息数量。
/// - block_duration: 阻塞超时时间。`Some(&Duration::ZERO)` 表示永久阻塞直到有新消息；
///   `Some(&Duration::from_millis(N))` 表示最多等待 N 毫秒，超时返回空结果；
///   `None` 表示非阻塞立即返回。
/// - group: 消费者组信息。`None` 表示普通读取；`Some((group_name, consumer_name))` 表示以消费者组方式读取。
/// - noack: 是否免应答，**仅在 `group` 为 `Some` 时有意义**（普通 `XREAD` 本来就没有 ACK 概念）。
///   `false` = 消息进入该消费者的 PEL，调用方处理完必须 `XACK`，否则会永久滞留 PEL；
///   `true` = 追加 `NOACK`，消息**不进入 PEL**，也就没有「待确认」状态，
///   调用方不需要（也不应该）再 `XACK`。代价是投递后未处理完就崩溃的消息会直接丢失。
/// 返回 `StreamReadReply` 结构体，无新消息时其中的 stream 列表为空。
pub async fn read_from_stream(
    stream_key: &str,
    last_id: &str,
    count: usize,
    block_duration: &Option<Duration>,
    group: Option<(&str, &str)>,
    noack: bool,
) -> Result<StreamReadReply, RedisError> {
    let mut conn = get_redis_conn()?;
    let mut opts = StreamReadOptions::default().count(count);
    if let Some(duration) = block_duration {
        opts = opts.block(duration.as_millis() as usize);
    }
    if let Some((group_name, consumer_name)) = group {
        opts = opts.group(group_name, consumer_name);
        if noack {
            opts = opts.noack();
        }
    }
    conn.xread_options(&[stream_key], &[last_id], &opts).await
}

/// # 创建消费者组
/// 执行 XGROUP CREATE MKSTREAM 命令。若流不存在则自动创建空流。
/// 组已存在时会返回错误，如需幂等创建请使用 ensure_consumer_group。
/// - stream_key: 流的键名。
/// - group_name: 消费者组名称。
/// - last_id: 组的起始消费位点，语义与 XGROUP CREATE 的 `$` 参数一致：
///   - `"$"` — 只消费建组之后新增的消息，流中现存消息一律跳过（适合「只关心新消息」的场景）；
///   - `"0"` — 把流中现存的所有消息都视为未投递，建组后会被逐一投递给组内消费者
///     （适合「流里每一条都必须被处理一次」的任务分发场景，重复投递由调用方自行幂等）。
///   该位点只在建组那一刻生效，组已存在时不起作用。
/// 无返回值，创建成功返回 Ok(())。
pub async fn create_consumer_group(
    stream_key: &str,
    group_name: &str,
    last_id: &str,
) -> Result<(), RedisError> {
    let mut conn = get_redis_conn()?;
    conn.xgroup_create_mkstream(stream_key, group_name, last_id)
        .await
}

/// # 幂等创建消费者组
/// 内部调用 create_consumer_group，若消费者组已存在（BUSYGROUP 错误）则忽略，
/// 其他错误正常返回。可反复调用，适合在读取报 NOGROUP 后就地自愈。
/// - stream_key: 流的键名。
/// - group_name: 消费者组名称。
/// - last_id: 组的起始消费位点，含义见 create_consumer_group。
/// 无返回值，创建成功或组已存在时返回 Ok(())。
pub async fn ensure_consumer_group(
    stream_key: &str,
    group_name: &str,
    last_id: &str,
) -> Result<(), RedisError> {
    match create_consumer_group(stream_key, group_name, last_id).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let err_msg = e.to_string();
            if err_msg.contains("BUSYGROUP") {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// # 确认消息已处理
/// 执行 XACK 命令确认指定消息已被当前消费者处理完毕，消息将从该消费者的 PEL 中移除。
/// - stream_key: 流的键名。
/// - group_name: 消费者组名称。
/// - message_ids: 待确认的消息 ID 切片。
/// 返回成功确认的消息数量。
pub async fn ack_stream_message(
    stream_key: &str,
    group_name: &str,
    message_ids: &[&str],
) -> Result<i64, RedisError> {
    let mut conn = get_redis_conn()?;
    conn.xack(stream_key, group_name, message_ids).await
}

/// # 获取流长度
/// 执行 XLEN 命令获取流中当前消息总数。
/// - stream_key: 流的键名。
/// 返回流中消息的数量。
pub async fn get_stream_length(stream_key: &str) -> Result<u64, RedisError> {
    let mut conn = get_redis_conn()?;
    conn.xlen(stream_key).await
}

/// # 删除流消息
/// 执行 XDEL 命令从流中删除指定消息。注意删除操作不会影响消费者组中已分发的消息。
/// - stream_key: 流的键名。
/// - message_ids: 待删除的消息 ID 切片。
/// 返回成功删除的消息数量。
pub async fn delete_stream_message(
    stream_key: &str,
    message_ids: &[&str],
) -> Result<i64, RedisError> {
    let mut conn = get_redis_conn()?;
    conn.xdel(stream_key, message_ids).await
}

/// # 按消息 ID 裁剪流
/// 执行 XTRIM MINID 命令删除所有 ID 小于 min_id 的消息。
/// 可利用消息 ID 包含毫秒时间戳的特性实现按时间清理，如 7 天前 = "{timestamp}-0"。
/// - stream_key: 流的键名。
/// - min_id: 最小保留的消息 ID，所有小于此 ID 的消息将被删除。
/// 返回被删除的消息数量。
pub async fn trim_stream_before(stream_key: &str, min_id: &str) -> Result<usize, RedisError> {
    let mut conn = get_redis_conn()?;
    let opts = StreamTrimOptions::minid(StreamTrimmingMode::Exact, min_id);
    conn.xtrim_options(stream_key, &opts).await
}

/// # 认领空闲超时的 pending 消息
/// 执行 XAUTOCLAIM 命令：把「已投递给组内某个消费者、但空闲时间超过 `min_idle_ms`
/// 仍未 ACK」的消息**改派**给 `consumer_name`，返回的 `claimed` 即为本次认领到的消息
/// （含完整字段，可直接交给业务处理）。
///
/// 典型用途是「消费者崩溃恢复」：某个 worker 被 kill / OOM / 网络分区时，
/// 它已从 `XREADGROUP ... ">"` 拿到但没来得及 ACK 的消息会永久滞留在它的 PEL 里，
/// `">"` 不会再返回它们。用 XAUTOCLAIM 把这些消息接管过来重跑，是唯一的出路。
///
/// ⚠️ **`min_idle_ms` 必须大于「单条消息的正常处理时长」**，否则会把另一个**仍然存活**
/// 的消费者正在处理中的消息抢过来，造成同一条消息被并发处理两次。空闲时间是从
/// 「最后一次投递」算起的，与消费者是否还在运行无关。
///
/// 注意返回的 `next_stream_id` 是下次扫描的游标（本函数固定从 `"0-0"` 开始扫，
/// 单次只扫一遍），`deleted_ids` 是「已被 XTRIM/XDEL 删除、但仍在 PEL 里」的孤儿 ID，
/// Redis 已顺带把它们从 PEL 清掉，调用方忽略即可。
///
/// - stream_key: 流的键名。
/// - group_name: 消费者组名称。
/// - consumer_name: 认领者（通常传当前实例名）。
/// - min_idle_ms: 最小空闲时间（毫秒），只有空闲超过该值的消息才会被认领。
/// - count: 单次最多认领的消息数量。
pub async fn auto_claim_stale_messages(
    stream_key: &str,
    group_name: &str,
    consumer_name: &str,
    min_idle_ms: usize,
    count: usize,
) -> Result<StreamAutoClaimReply, RedisError> {
    let mut conn = get_redis_conn()?;
    let opts = StreamAutoClaimOptions::default().count(count);
    conn.xautoclaim_options(
        stream_key,
        group_name,
        consumer_name,
        min_idle_ms,
        "0-0",
        opts,
    )
    .await
}

/// # 列出消费者组内的消费者
/// 执行 XINFO CONSUMERS 命令，返回组内每个消费者的名称、待处理（pending）消息数与空闲毫秒数。
///
/// 用于识别「已经死掉、但仍在组里占位」的消费者条目：`XREADGROUP` 会为每个出现过的
/// 消费者名自动建档，而 Redis **不会**因为进程退出就自动清理它，只能靠调用方判断
/// 后显式调用 [`delete_group_consumer`] 回收，否则消费者列表会随每次部署无限增长。
///
/// - stream_key: 流的键名。
/// - group_name: 消费者组名称。
pub async fn list_group_consumers(
    stream_key: &str,
    group_name: &str,
) -> Result<Vec<StreamInfoConsumer>, RedisError> {
    let mut conn = get_redis_conn()?;
    let reply: StreamInfoConsumersReply = conn.xinfo_consumers(stream_key, group_name).await?;
    Ok(reply.consumers)
}

/// # 删除消费者
/// 执行 XGROUP DELCONSUMER 命令，从消费者组中移除指定消费者。
///
/// ⚠️ 该操作会**连同该消费者的 PEL 一起丢弃** —— 它名下所有未 ACK 的消息就此失去归属，
/// 再也不会被任何消费者读到（除非消息本身还在流里且被重新发布）。因此调用前必须确认
/// `pending == 0`，或已先用 [`auto_claim_stale_messages`] 把待处理消息接管走。
///
/// 返回被丢弃的 pending 消息数量（正常回收时应为 0，非 0 说明有消息被连带丢弃）。
///
/// - stream_key: 流的键名。
/// - group_name: 消费者组名称。
/// - consumer_name: 待删除的消费者名称。
pub async fn delete_group_consumer(
    stream_key: &str,
    group_name: &str,
    consumer_name: &str,
) -> Result<usize, RedisError> {
    let mut conn = get_redis_conn()?;
    conn.xgroup_delconsumer(stream_key, group_name, consumer_name)
        .await
}
