use crate::dao::DaoError;
use crate::dao::eo::UniqueKey;
use linkme::distributed_slice;
use std::collections::HashMap;
use std::sync::OnceLock;

/// # 唯一键注册表
///
/// 存储唯一键元数据，键同时兼容 PostgreSQL 与 MySQL 的重复键错误格式，
/// 通过 [`init_unique_keys`] 完成初始化，供 DAO 层错误解析使用。
pub static UNIQUE_KEYS: OnceLock<HashMap<String, UniqueKey>> = OnceLock::new();

/// # 唯一键声明切片
///
/// 通过 [`distributed_slice`] 机制收集各实体模块声明的唯一键元组
/// `(表名, 键名, 键备注)`。
#[distributed_slice]
pub static UNIQUE_KEYS_SLICE: [(&str, &str, &str)];

/// # 初始化唯一键注册表
///
/// 遍历 `UNIQUE_KEYS_SLICE` 中声明的全部唯一键，构建唯一键元数据注册表。
///
/// ## 返回值
/// 初始化成功返回 `Ok(())`；注册表已初始化时返回 `DaoError::AlreadyInitialized`
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

/// # 按键查询唯一键元数据
///
/// 从唯一键注册表中查询指定键对应的唯一键元数据。
///
/// ## 参数
/// * `key` - 唯一键注册表中的键值，如 `ak_{键名}_{表名}` 或 `{表名}.ak_{键名}`
///
/// ## 返回值
/// 返回查询到的唯一键元数据；未找到时返回 `Ok(None)`
///
/// ## 错误
/// 唯一键注册表尚未初始化时返回 `DaoError::NotInitialized`
pub fn get_from_unique_keys(key: &str) -> Result<Option<&'static UniqueKey>, DaoError> {
    Ok(UNIQUE_KEYS
        .get()
        .ok_or_else(|| DaoError::NotInitialized("UNIQUE_KEYS未初始化".to_string()))?
        .get(key))
}

/// # 注册单个唯一键元数据
///
/// 构建唯一键元数据并插入到唯一键注册表中，同时为单个/复合键分别生成
/// PostgreSQL 与 MySQL 两种格式的键。
///
/// ## 参数
/// * `unique_keys` - 唯一键注册表
/// * `table` - 表名
/// * `key_name` - 键名，多个列以英文逗号分隔
/// * `key_remark` - 键备注
///
/// ## Panics
/// 当 `key_name` 按逗号拆分后为空（没有任何字段可用于唯一索引）时 panic
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
