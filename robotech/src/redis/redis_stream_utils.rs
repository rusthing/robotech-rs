use crate::redis::get_redis_conn;
use redis::{AsyncCommands, RedisError};

pub async fn publish_to_stream(
    stream_key: &str,
    fields: &[(&str, &str)],
) -> Result<String, RedisError> {
    let mut conn = get_redis_conn()?;
    conn.xadd(stream_key, "*", fields).await
}