/// # 当前用户 ID 请求头名称
///
/// 从 HTTP 请求头中读取当前用户 ID 时使用的头名称，值为 `"X-User-Id"`。
pub const USER_ID_HEADER_NAME: &str = "X-User-Id";

/// # 系统操作者用户 ID
///
/// 表示系统内部操作（非真实用户）使用的固定用户 ID。
pub const SYS_OPERATOR_USER_ID: u64 = 0;
