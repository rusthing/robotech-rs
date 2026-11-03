//! 变量来源抽象
//!
//! crate 只提供「按顺序查、先命中先返回」的机制，不决定业务优先级。
//! msg 传 `[内置, labels, annotations]`，alarm 传 `[labels, annotations]`，
//! 差异完全由调用方组装。

use std::collections::HashMap;

/// 只读变量查询
pub trait VariableLookup {
    /// 查一个变量；`None` 表示不存在
    fn lookup(&self, key: &str) -> Option<&str>;
}

impl VariableLookup for HashMap<String, String> {
    fn lookup(&self, key: &str) -> Option<&str> {
        self.get(key).map(String::as_str)
    }
}

impl VariableLookup for HashMap<&'static str, String> {
    fn lookup(&self, key: &str) -> Option<&str> {
        self.get(key).map(String::as_str)
    }
}

/// 按层优先的变量集
///
/// 第一个命中的层生效。层顺序就是业务优先级，调用方自己排。
pub struct PriorityVariables<'a> {
    layers: Vec<&'a dyn VariableLookup>,
}

impl<'a> PriorityVariables<'a> {
    /// 空集合
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// 追加一层（越早追加优先级越高）
    #[must_use]
    pub fn layer(mut self, layer: &'a dyn VariableLookup) -> Self {
        self.layers.push(layer);
        self
    }
}

impl Default for PriorityVariables<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableLookup for PriorityVariables<'_> {
    fn lookup(&self, key: &str) -> Option<&str> {
        self.layers.iter().find_map(|layer| layer.lookup(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_layer_wins() {
        let builtin: HashMap<&'static str, String> =
            HashMap::from([("k", "builtin".to_string())]);
        let labels = HashMap::from([("k".to_string(), "label".to_string())]);
        let vars = PriorityVariables::new().layer(&builtin).layer(&labels);
        assert_eq!(vars.lookup("k"), Some("builtin"));
    }

    #[test]
    fn falls_through_to_later_layer() {
        let labels: HashMap<String, String> = HashMap::new();
        let annotations = HashMap::from([("k".to_string(), "annotation".to_string())]);
        let vars = PriorityVariables::new().layer(&labels).layer(&annotations);
        assert_eq!(vars.lookup("k"), Some("annotation"));
    }
}
