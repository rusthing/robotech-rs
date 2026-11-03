use crate::api::U64;
use crate::api::validators::echo_value;
use crate::cst::user_id_cst::{CURRENT_MS_HEADER_NAME, USER_ID_HEADER_NAME};
use axum::http::HeaderMap;
use std::borrow::Cow;
use validator;

const ERR_MISSING: &str = "missing_header";
const ERR_FORMAT: &str = "invalid_header_format";

/// `X-User-Id` 的格式，写进错误提示
const USER_ID_EXPECTED: &str = "u64";

/// `X-Current-Ms` 的格式，写进错误提示
const CURRENT_MS_EXPECTED: &str = "毫秒时间戳";

/// # 构造「缺少必要请求头」的校验错误
///
/// 提示里带上格式：调用方漏了头的时候，顺手就知道该填什么。
fn missing_header_error(name: &str, expected: &str) -> validator::ValidationError {
    let msg = format!("缺少必要参数<{name}>，应为 {expected}");
    validator::ValidationError::new(ERR_MISSING).with_message(Cow::Owned(msg))
}

/// # 构造「请求头格式不正确」的校验错误
///
/// 提示里既写格式，也回显实际收到的值。头值不像 DTO 字段有字段名兜底
/// （`ValidationErrors` 会打出 `x_user_id: ...`），只有一句「格式不正确」时
/// 调用方既不知道该填什么，也不知道自己填了什么。
///
/// 头可能根本不是合法 UTF-8（`to_str` 会失败），所以先 lossy 转一遍再交给
/// [`echo_value`] 做转义与截断——非法字节显示成 `�`，总比什么都不说强。
fn invalid_header_error(name: &str, expected: &str, raw: &[u8]) -> validator::ValidationError {
    let msg = format!(
        "参数<{name}>格式不正确：应为 {expected}，实际收到 {}",
        echo_value(&String::from_utf8_lossy(raw))
    );
    validator::ValidationError::new(ERR_FORMAT).with_message(Cow::Owned(msg))
}

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
/// * 如果用户ID格式不正确，无法解析为u64类型，返回`ValidationError`；
///   提示里带期望格式与实际收到的值
pub fn get_current_user_id(headers: &HeaderMap) -> Result<U64, validator::ValidationError> {
    let raw = headers
        .get(USER_ID_HEADER_NAME)
        .ok_or_else(|| missing_header_error(USER_ID_HEADER_NAME, USER_ID_EXPECTED))?;
    // 两种失败都归到同一个错误码：`to_str` 失败是「有非法字节」，
    // `parse` 失败是「形态不对」，对调用方来说都是「这个头填得不对」，
    // 都按原始字节回显（`to_str` 失败时 text 根本取不到）。
    let text = raw
        .to_str()
        .map_err(|_| invalid_header_error(USER_ID_HEADER_NAME, USER_ID_EXPECTED, raw.as_bytes()))?;
    text.parse::<u64>()
        .map(U64)
        .map_err(|_| invalid_header_error(USER_ID_HEADER_NAME, USER_ID_EXPECTED, raw.as_bytes()))
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
/// * `Err(validator::ValidationError)` - 时间戳格式不正确时返回的校验错误；
///   提示里带期望格式与实际收到的值
pub fn get_current_ms(headers: &HeaderMap) -> Result<Option<U64>, validator::ValidationError> {
    let raw = match headers.get(CURRENT_MS_HEADER_NAME) {
        None => return Ok(None),
        Some(v) => v,
    };
    let text = raw.to_str().map_err(|_| {
        invalid_header_error(CURRENT_MS_HEADER_NAME, CURRENT_MS_EXPECTED, raw.as_bytes())
    })?;
    text.parse::<u64>().map(|ms| Some(U64(ms))).map_err(|_| {
        invalid_header_error(CURRENT_MS_HEADER_NAME, CURRENT_MS_EXPECTED, raw.as_bytes())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(name: &'static str, value: &[u8]) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(name, HeaderValue::from_bytes(value).unwrap());
        map
    }

    fn message(err: validator::ValidationError) -> String {
        err.message.unwrap().into_owned()
    }

    #[test]
    fn user_id_ok() {
        let r = get_current_user_id(&headers(USER_ID_HEADER_NAME, b"10086")).unwrap();
        assert_eq!(r, U64(10086));
    }

    #[test]
    fn user_id_missing_says_expected() {
        let msg = message(get_current_user_id(&HeaderMap::new()).unwrap_err());
        assert!(msg.contains("缺少必要参数<X-User-Id>"), "{msg}");
        assert!(msg.contains(USER_ID_EXPECTED), "{msg}");
    }

    #[test]
    fn user_id_echoes_actual_value() {
        // 这就是线上那条日志缺的东西：填了什么、该填什么
        let msg = message(get_current_user_id(&headers(USER_ID_HEADER_NAME, b"abc")).unwrap_err());
        assert_eq!(msg, "参数<X-User-Id>格式不正确：应为 u64，实际收到 \"abc\"");
    }

    #[test]
    fn user_id_echoes_non_utf8() {
        // 非法 UTF-8 走的是 to_str 失败那条分支，同样要回显
        let msg =
            message(get_current_user_id(&headers(USER_ID_HEADER_NAME, b"\xff\xfe")).unwrap_err());
        assert!(msg.contains("实际收到"), "{msg}");
    }

    #[test]
    fn user_id_truncates_long_value() {
        let raw = vec![b'x'; 200];
        let msg = message(get_current_user_id(&headers(USER_ID_HEADER_NAME, &raw)).unwrap_err());
        assert!(msg.contains("共 200 字符"), "{msg}");
        // 截断后只留 32 个字符：日志行长度不被请求头绑架
        assert!(msg.contains(&"x".repeat(32)), "{msg}");
        assert!(!msg.contains(&"x".repeat(33)), "{msg}");
    }

    #[test]
    fn current_ms_absent_is_ok() {
        assert_eq!(get_current_ms(&HeaderMap::new()).unwrap(), None);
    }

    #[test]
    fn current_ms_ok_and_bad() {
        let ok = get_current_ms(&headers(CURRENT_MS_HEADER_NAME, b"1751600000000")).unwrap();
        assert_eq!(ok, Some(U64(1751600000000)));
        let msg =
            message(get_current_ms(&headers(CURRENT_MS_HEADER_NAME, b"2026-11-03")).unwrap_err());
        assert!(msg.contains("实际收到 \"2026-11-03\""), "{msg}");
    }
}
