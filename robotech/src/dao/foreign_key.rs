use std::fmt;
use std::fmt::{Display, Formatter};
use typed_builder::TypedBuilder;

#[derive(Debug, Clone, TypedBuilder)]
#[builder]
pub struct ForeignKey {
    pub fk_table: String,
    pub fk_column: String,
    pub fk_table_comment: String,
    pub pk_table: String,
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
