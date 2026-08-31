use config::FileFormat;
use serde::{Deserialize, Serialize};

/// 配置的定位信息：namespace（环境/租户隔离）+ group（分组）+ data_id（配置项标识）。
///
/// 三段式借用了 Nacos 的概念，是三个后端里语义最丰富的一个；Consul / etcd 的适配器
/// 会把这三段拼接成各自的 key 路径（见对应 backend 模块里的 `build_key`）。
/// 这样上层业务代码只需要认识这一个结构体，不需要关心它最终落到哪种 key 形态上。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConfigKey {
    /// 命名空间(Nacos有此概念，Consul / etcd 等同于key的前缀)
    pub namespace: Option<String>,
    /// 分组(Nacos有此概念，Consul / etcd 等同于key的二级前缀)
    /// 一般用环境来分组，例如 `dev`、`test`、`prod`等
    pub group: Option<String>,
    /// 不带扩展名的配置项标识(Nacos有此概念，Consul / etcd 等同于key不带扩展名的最后一级)
    pub data_id: String,
}

impl ConfigKey {
    pub fn new(
        namespace: Option<impl Into<String>>,
        group: Option<impl Into<String>>,
        data_id: impl Into<String>,
    ) -> Self {
        Self {
            namespace: namespace.map(|v| v.into()),
            group: group.map(|v| v.into()),
            data_id: data_id.into(),
        }
    }

    /// 按 data_id 的后缀猜测格式，例如 `db-config.yaml` -> Yaml、`app.toml` -> Toml
    /// 这是三个后端共用的默认推断规则（各 backend 适配器构造 `ConfigItem` 时调用它）
    /// 猜不出来则归为 `None`
    pub fn infer_file_format(&self) -> Option<FileFormat> {
        let lower = self.data_id.to_ascii_lowercase();
        match lower.as_str() {
            s if s.ends_with(".toml") => Some(FileFormat::Toml),
            s if s.ends_with(".json") || s.ends_with(".json5") => Some(FileFormat::Json5),
            s if s.ends_with(".yml") || s.ends_with(".yaml") => Some(FileFormat::Yaml),
            s if s.ends_with(".ini") => Some(FileFormat::Ini),
            s if s.ends_with(".ron") => Some(FileFormat::Ron),
            _ => None,
        }
    }
}

impl std::fmt::Display for ConfigKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let namespace = if let Some(namespace) = &self.namespace {
            format!("{namespace}/")
        } else {
            "".to_string()
        };
        let group = if let Some(group) = &self.group {
            format!("{group}/")
        } else {
            "".to_string()
        };
        let data_id = &self.data_id;

        write!(f, "{}{}{}", namespace, group, data_id)
    }
}

/// 一份配置的完整内容 + 元信息。
#[derive(Debug, Clone)]
pub struct ConfigItem {
    pub key: ConfigKey,
    pub format: FileFormat,
    pub content: String,
}