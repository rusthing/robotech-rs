//! # 数据范围（Data Scope）注入
//!
//! **机制在框架、语义在业务**：框架只负责把业务注册的策略产出的额外条件，
//! 叠加到 `#[dao]` 生成的所有读写查询上；「当前用户是谁、能看到哪些范围」
//! 由业务侧自行决定（例如 adm-svr 用 task_local 保存请求级上下文），框架
//! 不绑定任何接入层的身份来源。
//!
//! ## 使用方式
//!
//! 1. 业务实现 [`DataScope`]，在 `condition(table)` 里按表名返回要追加的条件；
//!    `None` 表示该表不受限（应用、菜单、角色、权限这类全局资产可返回 `None`）。
//! 2. 启动时调用 [`set_data_scope`] 注册一次。
//! 3. `#[dao]` 生成的方法会自动调用 [`data_scope_condition`]，无需改业务代码。
//!
//! 未注册策略时所有函数都是透明的：`data_scope_condition` 原样返回入参条件，
//! [`has_data_scope`] 恒为 `false`，已有服务行为完全不变。

use sea_orm::Condition;
use std::sync::{Arc, OnceLock};

/// 全局唯一的数据范围策略
static DATA_SCOPE: OnceLock<Arc<dyn DataScope>> = OnceLock::new();

/// # 数据范围策略
///
/// `condition` 必须是**同步且无副作用**的：它会在每次查询构造时被调用，
/// 需要查库的计算应在接入层（中间件）完成并缓存到请求上下文中。
pub trait DataScope: Send + Sync + 'static {
    /// # 返回某张表需要追加的过滤条件
    ///
    /// ## 参数
    /// * `table` - 表名，由 `#[dao]` 宏按实体模块名生成
    ///
    /// ## 返回值
    /// * `Some(condition)` - 与原条件取 AND 后执行
    /// * `None` - 该表不受数据范围限制
    fn condition(&self, table: &str) -> Option<Condition>;
}

/// # 注册全局数据范围策略
///
/// 只允许注册一次；重复注册返回尚未插入的策略（调用方可据此告警）。
/// 建议在服务装配（`setup`）阶段、数据库建连之后调用。
pub fn set_data_scope(scope: impl DataScope) -> Result<(), Arc<dyn DataScope>> {
    let boxed: Arc<dyn DataScope> = Arc::new(scope);
    DATA_SCOPE.set(boxed)
}

/// # 某张表当前是否启用了数据范围
///
/// 供 `#[dao]` 生成代码判断是否需要额外的范围校验查询，避免无策略时
/// 每条更新/删除都多打一次库。
pub fn has_data_scope(table: &str) -> bool {
    DATA_SCOPE
        .get()
        .is_some_and(|scope| scope.condition(table).is_some())
}

/// # 把数据范围条件叠加到原条件
///
/// 未注册策略、或该表不受限时，原样返回 `condition`。
pub fn data_scope_condition(table: &str, condition: Condition) -> Condition {
    merge_condition(DATA_SCOPE.get().map(|scope| scope.as_ref()), table, condition)
}

/// # 只取数据范围条件（用于按主键查询等没有原条件的路径）
pub fn data_scope_only(table: &str) -> Condition {
    data_scope_condition(table, Condition::all())
}

/// # 叠加策略条件（纯函数）
///
/// 单独抽出来是为了让单测不必触碰全局 `OnceLock`：注册是进程级的一次性动作，
/// 在测试里注册会污染同一测试二进制里的其它用例。
fn merge_condition(
    scope: Option<&dyn DataScope>,
    table: &str,
    condition: Condition,
) -> Condition {
    match scope.and_then(|scope| scope.condition(table)) {
        Some(extra) => condition.add(extra),
        None => condition,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::sea_query::{Alias, Expr, ExprTrait, PostgresQueryBuilder, SelectStatement};

    struct TestScope;

    impl DataScope for TestScope {
        fn condition(&self, table: &str) -> Option<Condition> {
            match table {
                "adm_user" => Some(Condition::all().add(Expr::col(Alias::new("org_id")).eq(7))),
                _ => None,
            }
        }
    }

    fn sql_of(condition: Condition) -> String {
        let mut stmt = SelectStatement::new();
        stmt.from(Alias::new("adm_user")).cond_where(condition);
        stmt.to_string(PostgresQueryBuilder)
    }

    fn base_condition() -> Condition {
        Condition::all().add(Expr::col(Alias::new("enabled")).eq(true))
    }

    #[test]
    fn no_policy_is_transparent() {
        let sql = sql_of(merge_condition(None, "adm_user", base_condition()));
        assert!(sql.contains("enabled"), "{sql}");
        assert!(!sql.contains("org_id"), "{sql}");
    }

    #[test]
    fn unrelated_table_is_transparent() {
        let sql = sql_of(merge_condition(Some(&TestScope), "adm_role", base_condition()));
        assert!(!sql.contains("org_id"), "{sql}");
    }

    #[test]
    fn scoped_table_adds_and_condition() {
        let sql = sql_of(merge_condition(Some(&TestScope), "adm_user", base_condition()));
        assert!(sql.contains("enabled"), "{sql}");
        assert!(sql.contains("org_id"), "{sql}");
        assert!(sql.contains("AND"), "{sql}");
    }
}
