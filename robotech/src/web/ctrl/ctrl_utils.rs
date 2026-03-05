use crate::cst::user_id_cst::USER_ID_HEADER_NAME;
use axum::http::HeaderMap;
use validator;

/// # 从HTTP请求头中获取当前用户ID
///
/// 该函数会从请求头中提取用户ID，如果请求头中没有用户ID或格式不正确，
/// 将返回相应的ApiError错误。
///
/// ## 参数
///
/// * `req` - HTTP请求对象，包含请求头信息
///
/// ## 返回值
///
/// * `Ok(u64)` - 成功解析出的用户ID
/// * `Err(ApiError)` - 解析失败时返回的错误信息
///
/// ## 错误处理
///
/// * 如果请求头中缺少必要的用户ID参数，返回`ValidationError`
/// * 如果用户ID格式不正确，无法解析为u64类型，返回`ValidationError`
#[deprecated]
pub fn get_current_user_id(headers: &HeaderMap) -> Result<u64, validator::ValidationError> {
    headers
        .get(USER_ID_HEADER_NAME)
        .ok_or_else(|| {
            let message = format!("缺少必要参数<{}>", USER_ID_HEADER_NAME);
            validator::ValidationError::new(Box::leak(message.into_boxed_str()))
        })?
        .to_str()
        .map_err(|_| {
            let message = format!("参数<{}>格式不正确", USER_ID_HEADER_NAME);
            validator::ValidationError::new(Box::leak(message.into_boxed_str()))
        })?
        .parse::<u64>()
        .map_err(|_| {
            let message = format!("参数<{}>格式不正确", USER_ID_HEADER_NAME);
            validator::ValidationError::new(Box::leak(message.into_boxed_str()))
        })
}
