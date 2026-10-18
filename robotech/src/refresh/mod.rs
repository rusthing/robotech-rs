//! # 刷新信号
//!
//! 从本地缓存中抽出的通用「变更信号」能力：任意节点发布一条信号，所有订阅
//! 节点收到后执行自己的动作。
//!
//! 它与缓存没有必然关系——缓存只是其中一种用法：
//!
//! | 用法 | 收到信号后做什么 |
//! |------|------------------|
//! | 本地缓存 | 全量查库 → 构建快照 → `ArcSwap` 原子替换 |
//! | 队列订阅（如 msg-svr） | 查库 → 对比订阅 → 启动/停止 NATS 订阅 |
//!
//! ## 主题命名
//!
//! 约定 `refresh.{服务}.{刷新对象}`，一个刷新对象一个主题：
//!
//! | 服务 | 主题 | 刷新对象 |
//! |------|------|----------|
//! | msg | `refresh.msg.message` | 消息及其投递配置缓存 |
//! | msg | `refresh.msg.queue` | 队列订阅（非缓存） |
//! | dtw | `refresh.dtw.control-point-group` | 控制点分组缓存 |
//!
//! - 第一段固定为 `refresh`，标识「刷新信号」这一框架能力。与 NATS 自身的
//!   `$SYS.`（监控）、`$JS.API.`（JetStream API）一样按能力归类，而不是
//!   按业务归类：刷新是 robotech 提供的能力，会被所有服务复用，主题理应在
//!   同一命名空间下聚成一族；
//! - 与业务主题（`msg.message` / `msg.delivery`）完全隔离，不会被业务侧的
//!   通配 Stream 吞掉；
//! - 第二段为服务名。**`refresh.` 是框架保留前缀，业务主题不得占用**，
//!   也禁止创建 `refresh.>` 之类的通配 Stream——那会把所有服务的刷新信号
//!   收进同一个 Stream，导致各自的 consumer 收不到；
//! - 末段标识刷新对象：同一服务新增缓存时只追加末段，各自独立订阅，
//!   避免「一个缓存变更触发全部重建」；
//! - 末段内多词用 `-` 连接。Stream 名由主题把 `.` 换成 `_` 派生
//!   （如 `refresh_msg_message`），若 token 内用 `_`，`a.b_c` 与 `a_b.c`
//!   会派生出同一个 Stream 名。
//!
//! ### 用类型约束，不要手写字符串
//!
//! 主题由 [`RefreshSubject`] 承载，只能经 [`RefreshSubject::from_static`]
//! （通常再套一层
//! [`refresh_subject!`](crate::refresh_subject) 宏）构造：段数、前缀、字符集
//! 全部在 `const` 上下文校验，写错直接编译失败。
//!
//! ```ignore
//! use robotech::refresh::RefreshSubject;
//!
//! pub const MSG_MESSAGE_REFRESH_SUBJECT: RefreshSubject =
//!     robotech::refresh_subject!("msg", "message"); // => refresh.msg.message
//! ```
//!
//! 发布与订阅入口只接收 `&RefreshSubject`，调用方无法临时拼一个字符串绕过
//! 校验。
//!
//! ### 回调命名
//!
//! 业务侧「写成功后发布信号」的函数按 `on_{刷新对象}_refresh` 命名，刷新对象
//! 与主题末段对应；若是缓存，再加 `_cache`：
//!
//! | 函数 | 对应主题 |
//! |------|----------|
//! | `on_message_cache_refresh` | `refresh.msg.message` |
//! | `on_queue_refresh` | `refresh.msg.queue` |
//! | `on_control_point_group_cache_refresh` | `refresh.dtw.control-point-group` |
//!
//! 不要用笼统的 `on_cache_refresh`：一个服务一旦有第二份缓存，名字就区分不开，
//! `#[svc(after_write = ...)]` 挂错回调时也不容易发现。
//!
//! ## 可靠性约定
//!
//! 以下为基于 NATS JetStream 的实现（`nats` feature）：
//!
//! 信号可以丢：所有动作都是「全量重建」，丢几次不影响最终一致，只要事后
//! 补一次即可。因此订阅层不追求消息不丢，而是保证「断了能被发现并补齐」：
//!
//! - 建订阅后起一个守护任务，按 `[refresh] probe-interval` 周期探活；
//! - consumer 丢失（NATS 重启、断连超过 `inactive_threshold`、被人工删除）：
//!   重建订阅并立即执行一次动作，补齐中断期间漏掉的变更；
//! - 探活本身失败（NATS 不可达）：保持现状，下一轮再试，避免抖动误触发重建。
//!
//! ## 幂等
//!
//! `subscribe` 对同一 subject 只注册一次。配置热更新会重新执行 `setup()`，
//! 业务代码可以放心重复调用，不会叠加订阅。

pub mod config;
pub mod subject;

#[cfg(feature = "nats")]
pub mod nats;

pub use config::*;
pub use subject::RefreshSubject;

#[cfg(feature = "nats")]
pub use nats::{publish, subscribe};
