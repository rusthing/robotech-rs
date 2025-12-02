#[cfg(feature = "crud")]
use crate::svc::svc_error::SvcError::{DeleteViolateConstraint, DuplicateKey};
use log::error;
#[cfg(feature = "crud")]
use once_cell::sync::Lazy;
#[cfg(feature = "crud")]
use regex::{Captures, Regex};
#[cfg(feature = "crud")]
use sea_orm::DbErr;
#[cfg(feature = "crud")]
use std::collections::HashMap;

/// # 正则匹配重复键错误-Postgres
/// 格式: duplicate key value violates unique constraint "...", detail: Some("Key (<字段名>)=(<字段值>) already exists."), ...
#[cfg(feature = "crud")]
static REGEX_DUPLICATE_KEY_POSTGRES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"Key \((?P<column>[^)]+)\)=\((?P<value>[^)]*)\) already exists\."#).unwrap()
});

/// # 正则匹配重复键错误-MySQL
/// 格式: Duplicate entry '<字段值>' for key '<字段名>'
#[cfg(feature = "crud")]
static REGEX_DUPLICATE_KEY_MYSQL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"Duplicate entry '(?P<value>[^']+)' for key '(?P<column>[^']*)'$"#).unwrap()
});

/// # 正则匹配删除操作违反了约束条件错误-Postgres
/// 格式: Duplicate entry '<字段值>' for key '<字段名>'
#[cfg(feature = "crud")]
static REGEX_DELETE_VIOLATE_CONSTRAINT_POSTGRES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"update or delete on table \\"(?P<pk_table>[^"]+)\\" violates foreign key constraint \\"(?P<foreign_key>[^"]+)\\" on table \\"(?P<fk_table>[^"]+)\\""#).unwrap()
});

/// # 自定义服务层的错误枚举
///
/// 该枚举定义了服务层可能遇到的各种错误类型，包括数据未找到、重复键约束违反、
/// IO错误和数据库错误。这些错误类型用于在服务层统一处理各种异常情况，
/// 并提供清晰的错误信息反馈给调用方。
///
/// ## 错误类型说明
/// - `NotFound`: 表示请求的数据未找到，通常用于查询操作
/// - `DuplicateKey`: 表示违反了唯一性约束，如重复的用户名或邮箱
/// - `IoError`: 表示输入输出相关的错误，如文件读写失败
/// - `DatabaseError`: 表示底层数据库操作发生的错误
#[derive(Debug, thiserror::Error)]
pub enum SvcError {
    #[error("参数校验错误: {0}")]
    ValidationError(#[from] validator::ValidationError),
    #[error("参数校验错误: {0}")]
    ValidationErrors(#[from] validator::ValidationErrors),
    #[error("运行时错误: {0}")]
    RuntimeError(String),
    #[error("运行时错误: {0}")]
    RuntimeXError(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("找不到数据: {0}")]
    NotFound(String),
    #[error("IO错误: {0}")]
    IoError(#[from] std::io::Error),
    #[cfg(feature = "crud")]
    #[error("重复键错误: {0} {1}")]
    DuplicateKey(String, String),
    #[cfg(feature = "crud")]
    #[error("删除操作违反了数据库约束条件: {0} {1} {2}")]
    DeleteViolateConstraint(String, String, String),
    #[cfg(feature = "crud")]
    #[error("数据库错误: {0}")]
    DatabaseError(#[from] DbErr),
}

/// # 处理数据库错误，并转换为服务层错误
///
/// 该函数用于将数据库层的错误(DbErr)转换为服务层错误(SvcError)，
/// 特别处理了重复键错误，能够识别Postgres和MySQL的重复键错误格式，
/// 并将其转换为带有字段名称和值的DuplicateKey错误。
///
/// ## 参数
/// * `db_err` - 数据库错误对象
/// * `unique_field_hashmap` - 用于映射数据库列名到业务字段名的哈希表
///
/// ## 返回值
/// 返回对应的SvcError服务层错误对象
#[cfg(feature = "crud")]
pub fn handle_db_err_to_svc_error(
    db_err: DbErr,
    unique_field_hashmap: &Lazy<HashMap<&'static str, &'static str>>,
) -> SvcError {
    error!("数据库错误: {}", db_err);
    let db_err_string = format!("{:?}", db_err);

    if let Some(caps) = REGEX_DUPLICATE_KEY_POSTGRES.captures(&db_err_string) {
        // 正则匹配重复键错误-Postgres
        return to_duplicate_key(caps, unique_field_hashmap);
    } else if let Some(caps) = REGEX_DUPLICATE_KEY_MYSQL.captures(&db_err_string) {
        // 正则匹配重复键错误-MySQL
        return to_duplicate_key(caps, unique_field_hashmap);
    } else if let Some(caps) = REGEX_DELETE_VIOLATE_CONSTRAINT_POSTGRES.captures(&db_err_string) {
        let pk_table = caps["pk_table"].to_string();
        let foreign_key = caps["foreign_key"].to_string();
        let fk_table = caps["fk_table"].to_string();
        // 正则匹配删除操作违反了约束条件错误-Postgres
        return DeleteViolateConstraint(pk_table, foreign_key, fk_table);
    }

    SvcError::DatabaseError(db_err)
}

/// # 从正则匹配中抓取有用信息转换成重复键错误
///
/// 该函数用于从正则表达式匹配结果中提取重复键错误的相关信息，
/// 包括冲突的列名和值，并通过映射表转换为业务层的字段名，
/// 最终构造出一个包含字段名和冲突值的DuplicateKey服务错误。
///
/// ## 参数
/// * `caps` - 正则表达式匹配结果，包含column和value两个命名捕获组
/// * `unique_field_hashmap` - 数据库列名到业务字段名的映射表
///
/// ## 返回值
/// 返回一个包含字段名和冲突值的SvcError::DuplicateKey错误
#[cfg(feature = "crud")]
fn to_duplicate_key(
    caps: Captures,
    unique_field_hashmap: &Lazy<HashMap<&'static str, &'static str>>,
) -> SvcError {
    let column_name = caps["column"].to_string();
    let value = caps["value"].to_string();
    let name = unique_field_hashmap
        .get(column_name.as_str())
        .unwrap()
        .to_string();
    DuplicateKey(name, value)
}
