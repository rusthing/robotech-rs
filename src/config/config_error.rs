/// # 自定义配置错误的枚举
///
/// 该枚举定义了配置模块可能遇到的各种错误类型，包括文件系统监听错误和配置构建错误。
/// 这些错误类型用于在配置加载和处理过程中统一处理各种异常情况，
/// 并提供清晰的错误信息反馈给调用方。
///
/// ## 错误类型说明
/// - `NotifyError`: 表示文件系统监听相关的错误，如无法监听配置文件变化
/// - `BuildError`: 表示配置构建过程中的错误，如解析配置文件失败
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("通知错误: {0}")]
    NotifyError(#[from] notify::Error),
    #[error("构建配置错误: {0}")]
    BuildError(#[from] config::ConfigError),
}