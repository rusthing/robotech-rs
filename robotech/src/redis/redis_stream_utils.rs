use crate::redis::get_redis_conn;
use redis::streams::{StreamReadOptions, StreamReadReply, StreamTrimOptions, StreamTrimmingMode};
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

/// # 仅在 Stream 为空时发布消息
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
/// 注意：阻塞期间会独占 Redis 连接，请勿在高并发、连接池场景中使用；
/// 适合每个 Worker 持有独立连接的场景。
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
/// 返回 `StreamReadReply` 结构体，无新消息时其中的 stream 列表为空。
pub async fn read_from_stream(
    stream_key: &str,
    last_id: &str,
    count: usize,
    block_duration: &Option<Duration>,
    group: Option<(&str, &str)>,
) -> Result<StreamReadReply, RedisError> {
    let mut conn = get_redis_conn()?;
    let mut opts = StreamReadOptions::default().count(count);
    if let Some(duration) = block_duration {
        opts = opts.block(duration.as_millis() as usize);
    }
    if let Some((group_name, consumer_name)) = group {
        opts = opts.group(group_name, consumer_name);
    }
    conn.xread_options(&[stream_key], &[last_id], &opts).await
}

/// # 创建消费者组
/// 执行 XGROUP CREATE MKSTREAM 命令。若流不存在则自动创建空流。
/// 组已存在时会返回错误，如需幂等创建请使用 ensure_consumer_group。
/// - stream_key: 流的键名。
/// - group_name: 消费者组名称。
/// 无返回值，创建成功返回 Ok(())。
pub async fn create_consumer_group(stream_key: &str, group_name: &str) -> Result<(), RedisError> {
    let mut conn = get_redis_conn()?;
    conn.xgroup_create_mkstream(stream_key, group_name, "$")
        .await
}

/// # 幂等创建消费者组
/// 内部调用 create_consumer_group，若消费者组已存在（BUSYGROUP 错误）则忽略，
/// 其他错误正常返回。
/// - stream_key: 流的键名。
/// - group_name: 消费者组名称。
/// 无返回值，创建成功或组已存在时返回 Ok(())。
pub async fn ensure_consumer_group(stream_key: &str, group_name: &str) -> Result<(), RedisError> {
    match create_consumer_group(stream_key, group_name).await {
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