use crate::mq::nats::nats_error::NatsError;
use arc_swap::ArcSwapOption;
use async_nats::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wheel_rs::serde::vec_serde;

/// NATS 配置键
///
/// 用于标识配置文件中的 NATS 配置段，及配置变更检测。
pub const NATS_CONFIG_KEY: &str = "nats";

/// 全局 NATS 客户端
///
/// 通过 [`ArcSwapOption`] 实现无锁的运行时替换，配置热更新时无需重启即可切换连接。
static NATS_CLIENT: ArcSwapOption<Client> = ArcSwapOption::const_empty();

/// # 获取全局 NATS 客户端
///
/// 从全局 [`NATS_CLIENT`] 中加载当前连接，若尚未初始化则返回错误。
pub fn get_nats_client() -> Result<Client, NatsError> {
    NATS_CLIENT
        .load_full()
        .map(|arc| arc.as_ref().clone())
        .ok_or(NatsError::Connect("NATS 客户端未初始化".to_string()))
}

/// # 设置全局 NATS 客户端
///
/// 将新的 NATS 连接存入全局客户端，替换旧的连接。
pub fn set_nats_client(client: Client) {
    NATS_CLIENT.store(Some(Arc::new(client)));
}

/// # NATS 连接配置
///
/// 对应 `async_nats::ConnectOptions` 的全部可配置属性。
/// 未设置的字段使用 `ConnectOptions::default()` 对应的默认值。
///
/// ## 配置示例
///
/// ```yaml
/// nats:
///   servers:
///     - nats://localhost:4222
///   token: my-token
///   max-reconnects: 10
///   connection-timeout-ms: 5000
/// ```
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct NatsConfig {
    // ── 连接地址 ──
    /// NATS 服务器地址列表
    ///
    /// 支持多个地址实现集群容灾，如 `["nats://host1:4222", "nats://host2:4222"]`。
    #[serde(with = "vec_serde")]
    pub servers: Vec<String>,

    // ── 基础连接 ──
    /// 客户端名称
    ///
    /// 用于在 NATS 服务端标识当前连接，默认值 `"robotech"`。
    pub client_name: Option<String>,
    /// 连接认证用户名
    pub user: Option<String>,
    /// 连接认证密码
    pub password: Option<String>,
    /// 连接认证 Token
    pub token: Option<String>,
    /// 是否禁用回显
    ///
    /// 启用后本连接发送的消息不会回传给自身。
    pub no_echo: bool,
    /// 最大重连次数
    ///
    /// `None` 表示无限制重连。
    pub max_reconnects: Option<usize>,
    /// 连接超时，单位毫秒
    ///
    /// 默认值 5000。
    pub connection_timeout_ms: Option<u64>,
    /// 是否无限重试初始连接
    ///
    /// 默认 `true`，失败后会持续重试直到连接成功。
    pub retry_on_initial_connect: Option<bool>,

    // ── TLS ──
    /// 是否要求 TLS 连接
    pub tls_required: bool,
    /// 是否先建立 TLS 再发送协议信息
    pub tls_first: bool,
    /// TLS 根证书路径列表
    pub tls_root_certificates: Option<Vec<String>>,
    /// TLS 客户端证书路径
    pub tls_client_certificate: Option<String>,
    /// TLS 客户端私钥路径
    pub tls_client_key: Option<String>,

    // ── 心跳与超时 ──
    /// Ping 间隔，单位毫秒
    ///
    /// 用于服务端心跳检测，默认值 60000。
    pub ping_interval_ms: Option<u64>,
    /// 请求超时，单位毫秒
    ///
    /// 控制 request-reply 模式的等待时间，默认值 10000。
    pub request_timeout_ms: Option<u64>,

    // ── 缓冲区与容量 ──
    /// 订阅通道容量
    ///
    /// 控制订阅消息的内部缓冲大小，默认值 65536。
    pub subscription_capacity: Option<usize>,
    /// 发送通道容量
    ///
    /// 控制客户端发送消息的内部缓冲大小，默认值 2048。
    pub sender_capacity: Option<usize>,
    /// 读取缓冲区大小
    ///
    /// 控制读端缓冲区字节数，默认值 65535。
    pub read_buffer_capacity: Option<u16>,

    // ── 高级选项 ──
    /// 自定义收件箱前缀
    ///
    /// 默认值 `"_INBOX"`。
    pub inbox_prefix: Option<String>,
    /// 是否忽略集群发现的服务器
    pub ignore_discovered_servers: bool,
    /// 是否保留服务器连接顺序
    pub retain_servers_order: bool,
    /// 是否跳过 Subject 合法性校验
    pub skip_subject_validation: bool,
    /// 绑定本地地址
    ///
    /// 格式如 `"127.0.0.1:0"`。
    pub local_address: Option<String>,
}