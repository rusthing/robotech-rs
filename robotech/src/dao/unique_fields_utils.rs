use crate::dao::eo::UniqueField;
use crate::dao::DaoError;
use linkme::distributed_slice;
use std::collections::HashMap;
use std::sync::OnceLock;

pub static UNIQUE_FIELDS: OnceLock<HashMap<String, UniqueField>> = OnceLock::new();
#[distributed_slice]
pub static UNIQUE_FIELDS_SLICE: [(&str, &str, &str)];

pub fn init_unique_fields() -> Result<(), DaoError> {
    let mut entries = HashMap::new();
    for (table, column, column_comment) in UNIQUE_FIELDS_SLICE {
        push_unique_field(
            &mut entries,
            table.to_string(),
            column.to_string(),
            column_comment.to_string(),
        );
    }
    UNIQUE_FIELDS
        .set(entries)
        .map_err(|_| DaoError::AlreadyInitialized("UNIQUE_FIELDS已经初始化".to_string()))
}

pub fn get_from_unique_fields(key: &str) -> Result<Option<&'static UniqueField>, DaoError> {
    Ok(UNIQUE_FIELDS
        .get()
        .ok_or_else(|| DaoError::NotInitialized("UNIQUE_FIELDS未初始化".to_string()))?
        .get(key))
}

pub fn push_unique_field(
    unique_fields: &mut HashMap<String, UniqueField>,
    table: String,
    column: String,
    column_comment: String,
) {
    let columns: Vec<String> = column.split(',').map(|s| s.trim().to_string()).collect();
    if columns.len() == 0 {
        panic!("No fields provided for unique index")
    }
    let unique_field = UniqueField::builder()
        .table(table)
        .column(column)
        .column_comment(column_comment)
        .build();
    if columns.len() == 1 {
        // 添加postgre类的key
        let key = format!("ak_{}_{}", unique_field.column, unique_field.table);
        unique_fields.insert(key, unique_field.clone());
        // 添加mysql9类的key
        let key = format!("{}.ak_{}", unique_field.table, unique_field.column);
        unique_fields.insert(key, unique_field);
    } else {
        // 添加postgre类的key
        let key = format!("ak_{}_{}", columns.join("_and_"), unique_field.table);
        unique_fields.insert(key, unique_field.clone());
        // 添加mysql9类的key
        let key = format!("{}.ak_{}", unique_field.table, columns.join("_and_"));
        unique_fields.insert(key, unique_field);
    }
}
