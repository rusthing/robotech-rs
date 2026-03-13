use crate::dao::eo::ForeignKey;
use crate::dao::DaoError;
use linkme::distributed_slice;
use std::collections::HashMap;
use std::sync::OnceLock;

pub static FOREIGN_KEYS: OnceLock<HashMap<String, ForeignKey>> = OnceLock::new();

#[distributed_slice]
pub static FOREIGN_KEYS_SLICE: [(&str, &str, &str, &str, &str)];

pub fn init_foreign_keys() -> Result<(), DaoError> {
    let mut entries = HashMap::new();
    for (fk_table, fk_table_comment, fk_column, pk_table, pk_table_comment) in FOREIGN_KEYS_SLICE {
        push_foreign_key(
            &mut entries,
            fk_table.to_string(),
            fk_table_comment.to_string(),
            fk_column.to_string(),
            pk_table.to_string(),
            pk_table_comment.to_string(),
        );
    }
    FOREIGN_KEYS
        .set(entries)
        .map_err(|_| DaoError::AlreadyInitialized("FOREIGN_KEYS已经初始化".to_string()))
}

pub fn get_from_foreign_keys(key: &str) -> Result<Option<&'static ForeignKey>, DaoError> {
    Ok(FOREIGN_KEYS
        .get()
        .ok_or_else(|| DaoError::NotInitialized("FOREIGN_KEYS未初始化".to_string()))?
        .get(key))
}

pub fn calc_key_of_foreign_key(fk_table: &str, fk_column: &str, pk_table: &str) -> String {
    format!("{fk_table}_{fk_column}_{pk_table}")
}

pub fn push_foreign_key(
    foreign_keys: &mut HashMap<String, ForeignKey>,
    fk_table: String,
    fk_table_comment: String,
    fk_column: String,
    pk_table: String,
    pk_table_comment: String,
) {
    foreign_keys.insert(
        calc_key_of_foreign_key(&fk_table, &fk_column, &pk_table),
        ForeignKey::builder()
            .fk_table_comment(fk_table_comment)
            .fk_table(fk_table)
            .fk_column(fk_column)
            .pk_table_comment(pk_table_comment)
            .pk_table(pk_table)
            .build(),
    );
}
