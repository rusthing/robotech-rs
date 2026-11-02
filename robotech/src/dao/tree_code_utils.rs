//! # 树编码生成
//!
//! 树表（路径枚举模型，如 `adm_org` / `adm_menu`）新增节点时，服务端要算出
//! 「父节点下的下一个可用编码」。纯字符串递推在 `wheel_rs::tree_utils`，
//! 这里补上它缺的那一环：**查库拿当前最大的直接子编码**。
//!
//! ## 用法（服务钩子里）
//!
//! ```ignore
//! use robotech::dao::{next_tree_code, TreeCodeError};
//!
//! let code = next_tree_code(
//!     db,
//!     "adm_menu",           // 表名
//!     "tree_code",          // 树编码列名
//!     &parent,              // 父编码，空串 = 根
//!     &[("app_id", app_id.to_db_value())], // 同层隔离条件
//!     1,                    // 步长
//! ).await?;
//! ```
//!
//! ## 并发
//!
//! 两个请求同时挂到同一父节点下，可能算出同一个编码，其中一个会撞唯一约束
//! （`ak_..._tree_code`），由 DAO 的重复键解析把错误翻译给调用方。这是
//! 「读-算-写」的固有竞态，不做重试：撞了让调用方重试整个请求即可。
//!
//! ## 表名与列名
//!
//! `table` / `tree_col` / `scope` 的列名是**拼进 SQL 的**，不接受任何来自
//! 请求的数据——调用方一律传代码里的常量。

use sea_orm::sea_query::{Alias, Expr, Func, Query};
use sea_orm::{ConnectionTrait, DbErr, ExprTrait, Value};
use wheel_rs::tree_utils;

/// # 树编码生成错误
#[derive(Debug, thiserror::Error)]
pub enum TreeCodeError {
    /// 查库失败
    #[error("数据库错误: {0}")]
    Db(#[from] DbErr),
    /// 传进来的父编码不是合法的树编码（空串表示根，是合法的）
    #[error("父编码不合法: {0}")]
    InvalidParent(String),
    /// 该层兄弟数达到上限（999），或层数已达上限（10）
    #[error("树编码已满: 该层兄弟数达到上限或层数已达上限")]
    Exhausted,
}

/// # 生成父节点下的下一个子编码
///
/// 查出 `parent_code` 下已有的最大直接子编码，加上步长得到下一个可用编码。
/// 直接子节点的判定：以父编码为前缀，且长度恰好多一段（`LIKE '父%' AND
/// 长度 = 父长度 + 3`），孙节点不参与竞争。
///
/// ## 参数
/// * `db` - 数据库连接（服务钩子里拿到的事务连接即可，读操作不占额外事务）
/// * `table` - 表名，代码常量
/// * `tree_col` - 树编码列名，代码常量
/// * `parent_code` - 父编码；空串表示根层
/// * `scope` - 同层隔离条件（列名, 值）列表，代码常量；无隔离传 `&[]`
/// * `step` - 段值步长，见 `wheel_rs::tree_utils::DEFAULT_STEP`
///
/// ## 返回值
/// * `Ok(String)` - 下一个可用的树编码
/// * `Err(TreeCodeError::InvalidParent)` - 父编码不是合法树编码
/// * `Err(TreeCodeError::Exhausted)` - 该层已满，或层数已达上限
/// * `Err(TreeCodeError::Db)` - 查库失败
///
/// ## u64 作用域值
///
/// `scope` 里的值一律用 [`Value`]：u64 必须走 `U64::to_db_value()`（位模式
/// 绑 `BigInt`），裸 u64 会被 sea-query 当 `BigUnsigned`，绑参时超范围直接 panic。
pub async fn next_tree_code<C>(
    db: &C,
    table: &str,
    tree_col: &str,
    parent_code: &str,
    scope: &[(&str, Value)],
    step: u32,
) -> Result<String, TreeCodeError>
where
    C: ConnectionTrait + Sync,
{
    if !parent_code.is_empty() && !tree_utils::is_valid(parent_code) {
        return Err(TreeCodeError::InvalidParent(parent_code.to_string()));
    }

    let mut select = Query::select()
        .expr_as(Expr::col(Alias::new(tree_col)).max(), Alias::new("max_code"))
        .from(Alias::new(table))
        .and_where(Expr::col(Alias::new(tree_col)).like(format!("{parent_code}%")))
        .and_where(Expr::expr(Func::char_length(Expr::col(Alias::new(tree_col)))).eq(
            (parent_code.len() + tree_utils::LEVEL_LEN) as i32,
        ))
        .to_owned();
    for (scope_col, scope_value) in scope {
        select = select
            .and_where(Expr::col(Alias::new(*scope_col)).eq(scope_value.clone()))
            .to_owned();
    }

    let row = db.query_one(&select).await?;
    // 聚合查询恒返回一行；没有子节点时 max 为 NULL
    let max: Option<String> = match row {
        Some(row) => row.try_get("", "max_code")?,
        None => None,
    };

    tree_utils::next_child_code(parent_code, max.as_deref(), step)
        .ok_or(TreeCodeError::Exhausted)
}
