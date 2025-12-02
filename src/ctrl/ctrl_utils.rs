use crate::cst::user_id_cst::USER_ID_HEADER_NAME;
use actix_web::web::Query;
use actix_web::HttpRequest;
use std::collections::HashMap;
use validator;

/// # 从Query参数中获取ID
///
/// 该函数会从Query参数中提取ID，如果参数中没有ID或格式不正确，
/// 将返回相应的ValidationError错误。
///
/// ## 参数
///
/// * `query` - Query参数对象，包含ID参数
///
/// ## 返回值
///
/// * `Ok(u64)` - 成功解析出的ID
/// * `Err(ValidationError)` - 解析失败时返回的错误信息
///
/// ## 错误处理
///
/// * 如果Query参数中没有ID参数，返回`ValidationError`
pub fn get_id_from_query_params(
    query: Query<HashMap<String, String>>,
) -> actix_web::Result<u64, validator::ValidationError> {
    let id = match query.get("id") {
        Some(id_str) => match id_str.parse::<u64>() {
            Ok(id_val) => id_val,
            Err(_) => {
                let msg = format!("参数<id>格式错误: {}", id_str);
                return Err(validator::ValidationError::new(Box::leak(
                    msg.into_boxed_str(),
                )));
            }
        },
        None => {
            return Err(validator::ValidationError::new("缺少必要参数<id>"));
        }
    };
    Ok(id)
}

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
pub fn get_current_user_id(req: HttpRequest) -> Result<u64, validator::ValidationError> {
    req.headers()
        .get(USER_ID_HEADER_NAME)
        .ok_or_else(|| {
            let msg = format!("缺少必要参数<{}>", USER_ID_HEADER_NAME);
            validator::ValidationError::new(Box::leak(msg.into_boxed_str()))
        })?
        .to_str()
        .unwrap()
        .to_string()
        .parse::<u64>()
        .map_err(|_| {
            let msg = format!("参数<{}>格式不正确", USER_ID_HEADER_NAME);
            validator::ValidationError::new(Box::leak(msg.into_boxed_str()))
        })
}
