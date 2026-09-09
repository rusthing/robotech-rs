use crate::dao::{init_foreign_keys, init_unique_keys, DaoError};
use crate::db::get_db_conn;
use anyhow::anyhow;
use sea_orm::sea_query::{Expr, Func};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbConn,
    ExprTrait, QueryOrder, TransactionTrait,
};
use std::sync::Arc;

/// # 初始化数据访问层
///
/// 初始化唯一键注册表和外键注册表，供 DAO 层错误解析、约束校验等功能使用。
///
/// ## 返回值
/// 初始化成功返回 `Ok(())`；注册表初始化失败或已初始化时返回 `DaoError`
pub fn init_dao() -> Result<(), DaoError> {
    init_unique_keys()?;
    init_foreign_keys()?;
    Ok(())
}

/// # 获取数据库连接引用
///
/// 若传入的 `db` 已存在则直接返回；否则从全局数据库连接池中获取并返回。
///
/// ## 参数
/// * `db` - 可选的数据库连接引用，由调用方传入
///
/// ## 返回值
/// 返回数据库连接引用；全局连接未初始化时返回 `DaoError::GetDbConn`
pub fn unwrap_db<C>(db: Option<Arc<C>>) -> Result<Arc<C>, DaoError>
where
    C: ConnectionTrait,
    Arc<C>: From<Arc<DatabaseConnection>>,
{
    if let Some(db) = db {
        Ok(db)
    } else {
        get_db_conn()
            .map_err(|_| DaoError::GetDbConn())
            .map(|conn| conn.into())
    }
}

/// # 开启数据库事务
///
/// 基于给定的数据库连接开启一个新事务。
///
/// ## 参数
/// * `db` - 数据库连接引用
///
/// ## 返回值
/// 返回新开启的事务对象
///
/// ## 错误
/// 底层数据库开启事务失败时返回 `DaoError::Db`
pub async fn begin_transaction(db: &DbConn) -> Result<DatabaseTransaction, DaoError> {
    Ok(db.begin().await?)
}

/// # 提交数据库事务
///
/// 提交给定的事务，事务中的变更将被持久化到数据库。
///
/// ## 参数
/// * `db` - 待提交的事务对象
///
/// ## 返回值
/// 提交成功返回 `Ok(())`
///
/// ## 错误
/// 底层数据库提交事务失败时返回 `DaoError::Db`
pub async fn commit_transaction(db: DatabaseTransaction) -> Result<(), DaoError> {
    db.commit().await?;
    Ok(())
}

/// # 关键字多字段 OR 模糊查询
///
/// 在指定的多个字段上构造以 OR 连接的不区分大小写模糊匹配条件，
/// 用于实现一个关键字对多个列的联合模糊搜索。
///
/// ## 参数
/// * `keyword` - 查询关键字，将统一转为小写并包裹在 `%` 通配符中参与 LIKE 匹配
/// * `cols` - 参与模糊匹配的字段集合
///
/// ## 返回值
/// 返回由各字段 LIKE 条件以 OR 连接组成的 `Condition`
pub fn build_like_condition<T>(keyword: &str, cols: &[T]) -> Condition
where
    T: ColumnTrait,
{
    cols.into_iter().fold(Condition::any(), |condition, col| {
        condition.add(Func::lower(Expr::col(*col)).like(format!("%{}%", keyword.to_lowercase())))
    })
}

/// # 为查询追加排序条件
///
/// 解析 `order_by` 字符串并按顺序为查询追加升序/降序排序。
///
/// 排序字符串支持以英文逗号分隔多个排序字段，每个字段可附带 `:asc` 或 `:desc`
/// 后缀，未指定后缀时默认升序；字段名支持 `列名` 或 `表名.列名` 两种形式。
///
/// ## 参数
/// * `query` - 待追加排序条件的查询对象
/// * `order_by` - 可选的排序字符串，例如 `"created_at:desc,name"`；为 `None` 时原样返回
///
/// ## 返回值
/// 追加排序条件后的查询对象
///
/// ## 错误
/// 排序字段包含两个以上由 `.` 分隔的段时返回 `DaoError::Runtime`（`_order_by` 参数格式错误）
pub fn add_order_by<Q>(mut query: Q, order_by: &Option<String>) -> Result<Q, DaoError>
where
    Q: QueryOrder,
{
    if let Some(order_by) = order_by {
        for order_by in order_by.split(",") {
            let (col, order) = if order_by.trim().to_lowercase().ends_with(":desc") {
                (order_by.trim().replace(":desc", ""), false)
            } else {
                (order_by.trim().replace(":asc", ""), true)
            };
            let col_parts: Vec<&str> = col.split(".").collect();
            let col = if col_parts.len() == 1 {
                Expr::col(col)
            } else if col_parts.len() == 2 {
                Expr::col((col_parts[0].to_string(), col_parts[1].to_string()))
            } else {
                return Err(DaoError::from(anyhow!(format!(
                    "_order_by 参数的格式错误：{order_by}"
                ))));
            };

            if order {
                query = query.order_by_asc(col);
            } else {
                query = query.order_by_desc(col);
            }
        }
    }
    Ok(query)
}