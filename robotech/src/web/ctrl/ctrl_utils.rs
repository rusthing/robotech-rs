use crate::cst::user_id_cst::{CURRENT_MS_HEADER_NAME, USER_ID_HEADER_NAME};
use axum::http::HeaderMap;
use validator;
use crate::api::U64;

/// # 从 HTTP 请求头中获取当前用户ID
///
/// 该函数会从请求头中提取用户ID，如果请求头中没有用户ID或格式不正确，
/// 将返回相应的校验错误。
///
/// ## 参数
///
/// * `headers` - HTTP 请求头，应包含用户ID（头名见 `USER_ID_HEADER_NAME`）
///
/// ## 返回值
///
/// * `Ok(U64)` - 成功解析出的用户ID
/// * `Err(validator::ValidationError)` - 缺少用户ID或格式不正确时返回的校验错误
///
/// ## 错误处理
///
/// * 如果请求头中缺少必要的用户ID参数，返回`ValidationError`
/// * 如果用户ID格式不正确，无法解析为u64类型，返回`ValidationError`
pub fn get_current_user_id(headers: &HeaderMap) -> Result<U64, validator::ValidationError> {
    headers
        .get(USER_ID_HEADER_NAME)
        .ok_or_else(|| {
            let msg = format!("缺少必要参数<{}>", USER_ID_HEADER_NAME);
            validator::ValidationError::new(Box::leak(msg.into_boxed_str()))
        })?
        .to_str()
        .map_err(|_| {
            let msg = format!("参数<{}>格式不正确", USER_ID_HEADER_NAME);
            validator::ValidationError::new(Box::leak(msg.into_boxed_str()))
        })?
        .parse::<u64>()
        .map(U64)
        .map_err(|_| {
            let msg = format!("参数<{}>格式不正确", USER_ID_HEADER_NAME);
            validator::ValidationError::new(Box::leak(msg.into_boxed_str()))
        })
}

/// # 从 HTTP 请求头中获取当前时间戳
///
/// 该函数从请求头中提取当前时间戳（毫秒）。与用户ID不同，该头为可选项：
/// 请求头中不存在时返回 `None`，由服务端 dao 层自动填充当前时间；
/// 请求头存在但格式不正确时返回校验错误。
///
/// ## 参数
///
/// * `headers` - HTTP 请求头，可能包含时间戳（头名见 `CURRENT_MS_HEADER_NAME`）
///
/// ## 返回值
///
/// * `Ok(Some(U64))` - 成功解析出的时间戳
/// * `Ok(None)` - 请求头中未提供时间戳
/// * `Err(validator::ValidationError)` - 时间戳格式不正确时返回的校验错误
pub fn get_current_ms(headers: &HeaderMap) -> Result<Option<U64>, validator::ValidationError> {
    match headers.get(CURRENT_MS_HEADER_NAME) {
        None => Ok(None),
        Some(v) => v
            .to_str()
            .map_err(|_| {
                let msg = format!("参数<{}>格式不正确", CURRENT_MS_HEADER_NAME);
                validator::ValidationError::new(Box::leak(msg.into_boxed_str()))
            })?
            .parse::<u64>()
            .map(|ms| Some(U64(ms)))
            .map_err(|_| {
                let msg = format!("参数<{}>格式不正确", CURRENT_MS_HEADER_NAME);
                validator::ValidationError::new(Box::leak(msg.into_boxed_str()))
            }),
    }
}