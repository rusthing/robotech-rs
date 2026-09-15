use crate::redis::get_redis_conn;
use redis::streams::{
    StreamReadOptions, StreamReadReply, StreamTrimOptions, StreamTrimmingMode,
};
use redis::{AsyncCommands, RedisError};

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

/// # 统一读取流消息
/// 同时支持普通读取（XREAD）和消费者组读取（XREADGROUP）。
/// - stream_key: 流的键名。
/// - last_id: 起始消息 ID，"0" 表示从头开始，">" 表示仅接收新消息（仅消费者组模式有效）。
/// - count: 单次最多读取的消息数量。
/// - group: 消费者组信息。None 表示普通读取；Some((group_name, consumer_name)) 表示以消费者组方式读取。
/// 返回 StreamReadReply 结构体。
pub async fn read_from_stream(
    stream_key: &str,
    last_id: &str,
    count: usize,
    group: Option<(&str, &str)>,
) -> Result<StreamReadReply, RedisError> {
    let mut conn = get_redis_conn()?;
    let mut opts = StreamReadOptions::default().count(count);
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
pub async fn create_consumer_group(
    stream_key: &str,
    group_name: &str,
) -> Result<(), RedisError> {
    let mut conn = get_redis_conn()?;
    conn.xgroup_create_mkstream(stream_key, group_name, "$").await
}

/// # 幂等创建消费者组
/// 内部调用 create_consumer_group，若消费者组已存在（BUSYGROUP 错误）则忽略，
/// 其他错误正常返回。
/// - stream_key: 流的键名。
/// - group_name: 消费者组名称。
/// 无返回值，创建成功或组已存在时返回 Ok(())。
pub async fn ensure_consumer_group(
    stream_key: &str,
    group_name: &str,
) -> Result<(), RedisError> {
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
pub async fn trim_stream_before(
    stream_key: &str,
    min_id: &str,
) -> Result<usize, RedisError> {
    let mut conn = get_redis_conn()?;
    let opts = StreamTrimOptions::minid(StreamTrimmingMode::Exact, min_id);
    conn.xtrim_options(stream_key, &opts).await
}
