use std::fmt;
use std::fmt::{Display, Formatter};
use typed_builder::TypedBuilder;

#[derive(Debug, Clone, TypedBuilder)]
#[builder]
pub struct UniqueField {
    pub table: String,
    pub column: String,
    pub column_comment: String,
}

impl Display for UniqueField {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "table: {}, column: {} comment: {}",
            self.table, self.column, self.column_comment
        )
    }
}
