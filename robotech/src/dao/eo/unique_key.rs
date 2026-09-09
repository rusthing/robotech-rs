use std::fmt;
use std::fmt::{Display, Formatter};
use typed_builder::TypedBuilder;

/// # 唯一键元数据
///
/// 描述一张表上的唯一键（唯一索引）约束元信息，包含所属表名、键名与备注。
#[derive(Debug, Clone, TypedBuilder)]
#[builder]
pub struct UniqueKey {
    /// 唯一键所属表名
    pub table: String,
    /// 键名，多个列以英文逗号分隔
    pub key_name: String,
    /// 键备注
    pub key_remark: String,
}

impl Display for UniqueKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "table: {}, key name: {}, key remark: {}",
            self.table, self.key_name, self.key_remark
        )
    }
}
