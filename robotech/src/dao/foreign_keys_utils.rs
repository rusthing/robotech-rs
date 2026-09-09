use crate::dao::DaoError;
use crate::dao::eo::ForeignKey;
use linkme::distributed_slice;
use std::collections::HashMap;
use std::sync::OnceLock;

/// # 外键注册表
///
/// 存储以 [`calc_key_of_foreign_key`] 生成的键为索引的外键元数据，
/// 通过 [`init_foreign_keys`] 完成初始化，供 DAO 层错误解析使用。
pub static FOREIGN_KEYS: OnceLock<HashMap<String, ForeignKey>> = OnceLock::new();

/// # 外键声明切片
///
/// 通过 [`distributed_slice`] 机制收集各实体模块声明的外键元组
/// `(外键表, 外键表注释, 外键列, 主键表, 主键表注释)`。
#[distributed_slice]
pub static FOREIGN_KEYS_SLICE: [(&str, &str, &str, &str, &str)];

/// # 初始化外键注册表
///
/// 遍历 `FOREIGN_KEYS_SLICE` 中声明的全部外键，构建外键元数据注册表。
///
/// ## 返回值
/// 初始化成功返回 `Ok(())`；注册表已初始化时返回 `DaoError::AlreadyInitialized`
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

/// # 按键查询外键元数据
///
/// 从外键注册表中查询指定键对应的外键元数据。
///
/// ## 参数
/// * `key` - 由 [`calc_key_of_foreign_key`] 生成的外键键值
///
/// ## 返回值
/// 返回查询到的外键元数据；未找到时返回 `Ok(None)`
///
/// ## 错误
/// 外键注册表尚未初始化时返回 `DaoError::NotInitialized`
pub fn get_from_foreign_keys(key: &str) -> Result<Option<&'static ForeignKey>, DaoError> {
    Ok(FOREIGN_KEYS
        .get()
        .ok_or_else(|| DaoError::NotInitialized("FOREIGN_KEYS未初始化".to_string()))?
        .get(key))
}

/// # 计算外键注册表键
///
/// 由外键表名、外键列名与主键表名拼接生成外键在注册表中的唯一键。
///
/// ## 参数
/// * `fk_table` - 外键所在表名
/// * `fk_column` - 外键列名
/// * `pk_table` - 被引用的主键表名
///
/// ## 返回值
/// 形如 `{fk_table}_{fk_column}_{pk_table}` 的键字符串
pub fn calc_key_of_foreign_key(fk_table: &str, fk_column: &str, pk_table: &str) -> String {
    format!("{fk_table}_{fk_column}_{pk_table}")
}

/// # 注册单个外键元数据
///
/// 构建外键元数据并插入到外键注册表中，键由 [`calc_key_of_foreign_key`] 生成。
///
/// ## 参数
/// * `foreign_keys` - 外键注册表
/// * `fk_table` - 外键所在表名
/// * `fk_table_comment` - 外键所在表的注释
/// * `fk_column` - 外键列名
/// * `pk_table` - 被引用的主键表名
/// * `pk_table_comment` - 主键表的注释
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
