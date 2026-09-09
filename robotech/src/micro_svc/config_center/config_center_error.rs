use crate::micro_svc::config_center::ConfigKey;
use thiserror::Error;

/// 配置中心的统一错误类型。
///
/// 各后端适配器（Consul / Etcd / Nacos）返回的错误都会映射到本枚举的对应变体，
/// 上层业务代码据此区分"配置不存在 / 格式错误 / 连接失败"等不同场景。
#[derive(Debug, Error)]
pub enum ConfigCenterError {
    /// 指定的配置不存在
    #[error("配置未找到: {0}")]
    NotFound(ConfigKey),

    /// 无法根据 data_id 后缀推断配置文件格式
    #[error("无法识别的文件格式: {0}")]
    UnknownFileFormat(String),

    /// 配置内容解析失败（如 base64 解码、UTF-8 转换失败）
    #[error("配置内容解析失败: {0}")]
    Parse(String),

    /// 后端连接或请求失败
    #[error("后端连接/请求失败: {0}")]
    Connection(String),

    /// 本地快照读写失败
    #[error("本地快照读写失败: {0}")]
    Cache(String),

    /// 后端未启用（对应 Cargo feature 未开启）
    #[error("不支持的后端: {0}（对应 Cargo feature 未启用，检查 Cargo.toml 里的 features）")]
    BackendNotEnabled(String),

    /// 其它内部错误
    #[error("配置错误: {0}")]
    Internal(String),
}
