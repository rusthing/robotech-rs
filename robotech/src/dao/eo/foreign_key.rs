use std::fmt;
use std::fmt::{Display, Formatter};
use typed_builder::TypedBuilder;

/// # 外键元数据
///
/// 描述一条数据库外键约束的元信息，包含外键所在表及其注释、
/// 外键列名以及被引用的主键表及其注释。
#[derive(Debug, Clone, TypedBuilder)]
#[builder]
pub struct ForeignKey {
    /// 外键所在表名
    pub fk_table: String,
    /// 外键所在表的注释
    pub fk_table_comment: String,
    /// 外键列名
    pub fk_column: String,
    /// 被引用的主键表名
    pub pk_table: String,
    /// 被引用的主键表的注释
    pub pk_table_comment: String,
}

impl Display for ForeignKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}({}).{} -> {}({})",
            self.fk_table,
            self.fk_table_comment,
            self.fk_column,
            self.pk_table,
            self.pk_table_comment
        )
    }
}
