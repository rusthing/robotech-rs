//! # 刷新主题
//!
//! 把 `refresh.<模块>.<缓存 key>` 这条命名约定固化成类型，避免各服务手写
//! 字符串时拼错段数、用了 `_`、或把业务主题写进 `refresh.` 命名空间。
//!
//! ## 为什么是类型而不是 `&str`
//!
//! - **编译期校验**：[`RefreshSubject::from_static`] 是 `const fn`，在 const
//!   上下文里校验失败就是 `panic!`，也就是编译错误。常量定义处写错，服务
//!   根本编不过，而不是等到运行时才发现信号发不到；
//! - **入口收敛**：`refresh::publish` / `refresh::subscribe` 只接收
//!   `&RefreshSubject`，调用方无法临时拼一个字符串蒙混过关；
//! - **派生确定**：Stream 名由主题把 `.` 换成 `_` 派生，段结构固定后派生
//!   结果也固定，不会出现 `a.b_c` 与 `a_b.c` 撞成同一个 Stream 名。
//!
//! ## 用法
//!
//! ```ignore
//! use robotech::refresh::RefreshSubject;
//!
//! pub const MSG_MESSAGE_REFRESH_SUBJECT: RefreshSubject =
//!     robotech::refresh_subject!("msg", "message"); // => refresh.msg.message
//! ```
//!
//! 不要绕过宏手写字符串——宏至少能保证「前缀 + 段数 + `.` 位置」不会错，
//! 字符集校验由 [`RefreshSubject::from_static`] 兜底。

/// 刷新主题前缀
const PREFIX: &[u8] = b"refresh.";

/// 段总数：`refresh` + 模块 + 缓存 key
const SEGMENTS: usize = 3;

/// 最短合法长度：`refresh.a.b`
const MIN_LEN: usize = 11;

/// 最长合法长度
///
/// Stream 名上限 255，主题与派生名（`.` → `_`）等长，这里取同一个上限。
const MAX_LEN: usize = 255;

/// # 刷新主题
///
/// 形如 `refresh.<模块>.<缓存 key>` 的、经过校验的 NATS Subject。
///
/// 只能通过 [`from_static`](Self::from_static)（通常再套一层
/// [`refresh_subject!`](crate::refresh_subject) 宏）构造，保证拿到的值一定
/// 满足命名约定。
///
/// ## 约束
///
/// | 约束 | 说明 |
/// |------|------|
/// | 恰好三段 | `refresh` / 模块 / 缓存 key，不允许再分级 |
/// | 首段固定 `refresh` | 与业务主题分属不同命名空间 |
/// | 只含 `a-z` `0-9` `-` `.` | 不允许 `_`、通配符、大写 |
/// | 每段非空 | `refresh..x`、`refresh.x.` 均非法 |
/// | 长度 11..=255 | 与 JetStream Stream 名上限对齐 |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefreshSubject(&'static str);

impl RefreshSubject {
    /// # 校验并构造刷新主题
    ///
    /// ## 参数
    ///
    /// - `subject`：完整主题字符串，如 `refresh.msg.message`
    ///
    /// ## Panics
    ///
    /// 不满足命名约定时 `panic!`。在 `const` 上下文中调用（如定义常量）
    /// 时，就是编译错误。
    pub const fn from_static(subject: &'static str) -> Self {
        let bytes = subject.as_bytes();
        let len = bytes.len();

        if len < MIN_LEN || len > MAX_LEN {
            panic!("刷新主题长度不合法：应为 `refresh.<模块>.<缓存key>`，长度 11..=255");
        }

        let mut i = 0;
        let mut dots = 0;
        while i < len {
            match bytes[i] {
                b'.' => dots += 1,
                b'a'..=b'z' | b'0'..=b'9' | b'-' => {}
                _ => panic!(
                    "刷新主题只能包含小写字母、数字、`.` 和 `-`；\
                     末段多词请用 `-` 连接（`_` 会与 Stream 名派生规则冲突）"
                ),
            }
            i += 1;
        }

        if dots != SEGMENTS - 1 {
            panic!("刷新主题必须恰好三段：`refresh.<模块>.<缓存key>`");
        }

        let mut p = 0;
        while p < PREFIX.len() {
            if bytes[p] != PREFIX[p] {
                panic!("刷新主题必须以 `refresh.` 开头");
            }
            p += 1;
        }

        // 第二段、第三段都不能为空（`refresh..x` / `refresh.x.`）
        if bytes[PREFIX.len()] == b'.' || bytes[len - 1] == b'.' {
            panic!("刷新主题的模块名与缓存 key 都不能为空");
        }

        Self(subject)
    }

    /// 完整主题字符串
    pub const fn as_str(&self) -> &'static str {
        self.0
    }

    /// 由主题派生的 JetStream Stream 名（`.` → `_`）
    ///
    /// 例：`refresh.msg.message` → `refresh_msg_message`
    pub fn stream_name(&self) -> String {
        self.0.replace('.', "_")
    }
}

impl std::fmt::Display for RefreshSubject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl AsRef<str> for RefreshSubject {
    fn as_ref(&self) -> &str {
        self.0
    }
}

/// # 构造刷新主题
///
/// 在编译期把模块名与缓存 key 拼成 `refresh.<模块>.<缓存key>`，并交给
/// [`RefreshSubject::from_static`] 校验。参数必须是字面量，杜绝运行时拼接。
///
/// ```ignore
/// use robotech::refresh::RefreshSubject;
///
/// pub const MSG_QUEUE_REFRESH_SUBJECT: RefreshSubject =
///     robotech::refresh_subject!("msg", "queue");
/// ```
#[macro_export]
macro_rules! refresh_subject {
    ($module:literal, $cache_key:literal) => {
        $crate::refresh::RefreshSubject::from_static(::core::concat!(
            "refresh.",
            $module,
            ".",
            $cache_key
        ))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejected(subject: &'static str) -> bool {
        std::panic::catch_unwind(|| RefreshSubject::from_static(subject)).is_err()
    }

    #[test]
    fn accepts_well_formed_subjects() {
        assert_eq!(
            RefreshSubject::from_static("refresh.msg.message").as_str(),
            "refresh.msg.message"
        );
        assert_eq!(
            RefreshSubject::from_static("refresh.dtw.control-point-group").as_str(),
            "refresh.dtw.control-point-group"
        );
        // 最短合法值
        assert_eq!(
            RefreshSubject::from_static("refresh.a.b").as_str(),
            "refresh.a.b"
        );
    }

    #[test]
    fn stream_name_replaces_dots() {
        assert_eq!(
            RefreshSubject::from_static("refresh.msg.message").stream_name(),
            "refresh_msg_message"
        );
    }

    #[test]
    fn rejects_wrong_prefix() {
        assert!(rejected("msg.refresh.message"));
        assert!(rejected("refreshx.msg.message"));
    }

    #[test]
    fn rejects_wrong_segment_count() {
        assert!(rejected("refresh.msg"));
        assert!(rejected("refresh.msg.message.queue"));
    }

    #[test]
    fn rejects_empty_segment() {
        assert!(rejected("refresh..message"));
        assert!(rejected("refresh.msg."));
    }

    #[test]
    fn rejects_underscore_and_wildcards() {
        assert!(rejected("refresh.msg.message_cache"));
        assert!(rejected("refresh.*.message"));
        assert!(rejected("refresh.>.message"));
    }

    #[test]
    fn rejects_uppercase() {
        assert!(rejected("refresh.MSG.message"));
    }
}
