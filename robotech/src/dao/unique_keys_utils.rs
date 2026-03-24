use crate::dao::eo::UniqueKey;
use crate::dao::DaoError;
use linkme::distributed_slice;
use std::collections::HashMap;
use std::sync::OnceLock;

pub static UNIQUE_KEYS: OnceLock<HashMap<String, UniqueKey>> = OnceLock::new();
#[distributed_slice]
pub static UNIQUE_KEYS_SLICE: [(&str, &str, &str)];

pub fn init_unique_keys() -> Result<(), DaoError> {
    let mut entries = HashMap::new();
    for (table, key_name, key_remark) in UNIQUE_KEYS_SLICE {
        push_unique_key(
            &mut entries,
            table.to_string(),
            key_name.to_string(),
            key_remark.to_string(),
        );
    }
    UNIQUE_KEYS
        .set(entries)
        .map_err(|_| DaoError::AlreadyInitialized("UNIQUE_KEYS已经初始化".to_string()))
}

pub fn get_from_unique_keys(key: &str) -> Result<Option<&'static UniqueKey>, DaoError> {
    Ok(UNIQUE_KEYS
        .get()
        .ok_or_else(|| DaoError::NotInitialized("UNIQUE_KEYS未初始化".to_string()))?
        .get(key))
}

pub fn push_unique_key(
    unique_keys: &mut HashMap<String, UniqueKey>,
    table: String,
    key_name: String,
    key_remark: String,
) {
    let columns: Vec<String> = key_name.split(',').map(|s| s.trim().to_string()).collect();
    if columns.len() == 0 {
        panic!("No fields provided for unique index")
    }
    let unique_key = UniqueKey::builder()
        .table(table)
        .key_name(key_name)
        .key_remark(key_remark)
        .build();
    if columns.len() == 1 {
        // 添加postgre类的key
        let key = format!("ak_{}_{}", unique_key.key_name, unique_key.table);
        unique_keys.insert(key, unique_key.clone());
        // 添加mysql9类的key
        let key = format!("{}.ak_{}", unique_key.table, unique_key.key_name);
        unique_keys.insert(key, unique_key);
    } else {
        // 添加postgre类的key
        let key = format!("ak_{}_{}", columns.join("_and_"), unique_key.table);
        unique_keys.insert(key, unique_key.clone());
        // 添加mysql9类的key
        let key = format!("{}.ak_{}", unique_key.table, columns.join("_and_"));
        unique_keys.insert(key, unique_key);
    }
}
