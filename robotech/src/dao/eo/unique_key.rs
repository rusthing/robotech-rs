use std::fmt;
use std::fmt::{Display, Formatter};
use typed_builder::TypedBuilder;

#[derive(Debug, Clone, TypedBuilder)]
#[builder]
pub struct UniqueKey {
    pub table: String,
    pub key_name: String,
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
