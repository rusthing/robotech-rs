//! # 排序值服务端维护
//!
//! 排序值只由服务端维护，业务层不接收、也不信任客户端提交的顺序值。
//! 本模块把同一套规则抽成与具体表无关的通用实现，表名、列名、分组列
//! 全部由 [`FlatSort`] / [`TreeSort`] 描述，调用方只传代码里的常量。
//!
//! ## 平铺实体
//!
//! 使用一个整数排序列（默认 `sort_order`），按可选的「分组列」隔离
//! （如 `realm_id`、`group_id`）；没有分组列时全表一组。
//!
//! - **新增取 `max + 1`**：同组内追加到末尾，客户端无法插入到中间。
//!   取最大值前先拿分组级 advisory lock，空组从 `1` 开始；超过
//!   [`SORT_ORDER_MAX`] 时要求先走重排接口。
//! - **重排**：提交同组完整 id 顺序，事务内整体重算为 `1..n`，不留空洞、
//!   不重复；提交的 id 必须恰好覆盖该组全部记录，否则说明有并发增删，要求刷新。
//! - **删除取前移**：单条删除后，同组中排在它后面的记录整体 `-1`，
//!   后面的序号前移补位。批量删除按组扫描，只对「最大值不等于条数」
//!   或「序号有重复」的组整组重算一次，不逐条压缩。
//! - **改分组**：新分组追加 `max + 1`，旧分组中排在它后面的记录前移 `-1`，
//!   两组在同一事务内完成。
//!
//! ## 树形实体
//!
//! 使用路径枚举编码（默认 `tree_code`，每层 3 位数字），按「外层隔离列 +
//! 父编码」形成兄弟组（如菜单的 `app_id`）。新增节点在父节点下追加末位编码
//! （[`next_child_code`]，同样是 `max + 1`）；删除节点时同一事务内先由
//! [`delete_tree_descendants`] 按编码前缀删除全部子孙，再由业务 `del_by_id`
//! 删除节点、[`compact_tree_after_delete`] 前移同父兄弟；重排、删除压缩、
//! 改父节点都要级联更新整棵子树，因此按表串行化：先 [`tree_lock`] 拿表级
//! advisory lock。
//!
//! - **临时编码两阶段落位**：相邻兄弟互换时两个编码会同时占用对方位置，
//!   先把每个兄弟子树挪到 `~xx`（base36 两位）临时编码，再落到最终
//!   `父编码 + 001..n`，避免唯一键冲突。
//! - **孤儿保护**：删除后若某个待前移编码下面还挂着「已删除节点留下的孤儿
//!   后代」，压缩会把它挂到新的兄弟节点下，这种组保留空洞、不做压缩；
//!   重排遇到孤儿则直接拒绝，要求先清理。
//! - **上限**：同一父节点最多 [`DEFAULT_TREE_MAX_SIBLINGS`] 个兄弟（三位子段
//!   只到 999），编码总长不超过 [`DEFAULT_TREE_MAX_LENGTH`]（10 层 × 3）。
//! - **防环**：同一外层隔离域内改父时，目标父编码不能等于自己，也不能以
//!   自己为前缀（即不能挂到自己的后代下）。跨 scope 时两边编码不在同一
//!   命名空间，不做前缀防环。
//! - **编码上限**：生成结果超过 [`TreeSort::max_length`] 时直接报错并回滚。
//!
//! ## 并发口径
//!
//! 所有写操作都要求调用方在事务内执行，失败时整体回滚。并发通过
//! PostgreSQL 事务级 advisory lock（`pg_advisory_xact_lock`）串行化，
//! key 前缀为 `robotech_sort:`：
//!
//! - 平铺分组：`{table}:{group_col}={value}`、`{table}:*`、`{table}:{col}=null`；
//! - 树形：表级 `tree:{table}`，同一事务内重复加锁可重入。
//!
//! 平铺「改分组」的加锁顺序是：先无锁读旧分组，按 key 字符串排序拿两把
//! 分组锁，最后才 `FOR UPDATE` 锁行。反过来「先锁行、再等分组锁」会与并发
//! 压缩形成行锁 ↔ 分组锁的环形等待。拿到两把锁后会重读记录并校验分组未被
//! 并发改走，避免在错误的组上压缩。批量删除时组也按 key 字符串排序后逐组
//! 加锁，消除不同方向的并发等待。
//!
//! 树形实体的新增、级联删除、重排、删除压缩、改父共用同一把表级锁——临时
//! 编码只在事务内可见，但仍要避免同表不同父节点的两次重排互相踩到对方的
//! 临时编码；级联删除也需要与并发的重排/压缩串行化。

use crate::api::U64;
use crate::dao::{next_tree_code, DaoError};
use crate::svc::SvcError;
use anyhow::anyhow;
use sea_orm::{ConnectionTrait, DatabaseBackend, DbErr, QueryResult, Statement, Value};
use std::collections::{HashMap, HashSet};
use wheel_rs::time_utils::now_ms;

/// 平铺排序列允许的最大值（与 DTO `@range 0..=9999` 对齐）
pub const SORT_ORDER_MAX: i32 = 9999;
/// 树形编码每层的默认步长
pub const DEFAULT_TREE_STEP: usize = 1;
/// 树形编码的默认最大长度（10 层 × 3 位）
pub const DEFAULT_TREE_MAX_LENGTH: usize = 30;
/// 同一父节点下默认的最大兄弟数（三位子段的上限）
pub const DEFAULT_TREE_MAX_SIBLINGS: usize = 999;
/// 重排时临时编码（`~` + base36 两位）能容纳的兄弟数量上限
const TEMP_CODE_CAPACITY: usize = 36 * 36;

/// # 从 DTO / VO 取平铺分组值
///
/// `U64` 视为必定有值；`Option<U64>` 的 `None` 表示「无分组」（全表一组
/// 或该列为 `NULL` 的分组），`Some` 转为 `i64` 绑定参数。
pub trait SortGroupValue {
    /// 返回分组值；`None` 表示该分组列为 `NULL`
    fn sort_group(self) -> Option<i64>;
}

impl SortGroupValue for U64 {
    fn sort_group(self) -> Option<i64> {
        Some(self.value() as i64)
    }
}

impl SortGroupValue for Option<U64> {
    fn sort_group(self) -> Option<i64> {
        self.map(|value| value.value() as i64)
    }
}

impl SortGroupValue for Option<Option<U64>> {
    fn sort_group(self) -> Option<i64> {
        self.flatten().map(|value| value.value() as i64)
    }
}

/// # 从 DTO 取树形父编码
///
/// 语义用于区分「不改父节点」与「移动到根」：
/// - `Option<Option<String>>` 的 `None` → `None`（不改父节点）；
/// - `Some(None)` → `Some("")`（**明确移动到根**）；
/// - `Some(Some(s))` → `Some(s)`；
/// - `String` / `Option<String>` 直接视为已给出的父编码。
pub trait SortParentValue {
    /// 返回父编码；空串表示根
    fn sort_parent(self) -> Option<String>;
}

impl SortParentValue for String {
    fn sort_parent(self) -> Option<String> {
        Some(self)
    }
}

impl SortParentValue for Option<String> {
    fn sort_parent(self) -> Option<String> {
        self
    }
}

impl SortParentValue for Option<Option<String>> {
    fn sort_parent(self) -> Option<String> {
        self.map(|parent| parent.unwrap_or_default())
    }
}

/// # 平铺实体排序表描述
///
/// 所有字段都是代码常量，直接拼进 SQL，不接受任何来自请求的数据。
#[derive(Debug, Clone, Copy)]
pub struct FlatSort {
    /// 表名
    pub table: &'static str,
    /// 主键列名，默认 `id`
    pub id_col: &'static str,
    /// 排序列名，默认 `sort_order`
    pub sort_col: &'static str,
    /// 分组列名；`None` 表示全表一组
    pub group_col: Option<&'static str>,
    /// 操作人列名，默认 `updater_id`
    pub updater_col: &'static str,
    /// 更新时间列名，默认 `update_ms`
    pub update_ms_col: &'static str,
    /// 排序列最大值，默认 [`SORT_ORDER_MAX`]
    pub max: i32,
}

impl FlatSort {
    /// 用默认列名构造表描述
    pub const fn new(table: &'static str) -> Self {
        Self {
            table,
            id_col: "id",
            sort_col: "sort_order",
            group_col: None,
            updater_col: "updater_id",
            update_ms_col: "update_ms",
            max: SORT_ORDER_MAX,
        }
    }

    /// 指定分组列
    pub const fn with_group(mut self, col: &'static str) -> Self {
        self.group_col = Some(col);
        self
    }

    /// 指定排序列最大值（默认 [`SORT_ORDER_MAX`]）
    pub const fn with_max(mut self, max: i32) -> Self {
        self.max = max;
        self
    }
}

/// # 树形实体排序表描述
///
/// 所有字段都是代码常量，直接拼进 SQL，不接受任何来自请求的数据。
#[derive(Debug, Clone, Copy)]
pub struct TreeSort {
    /// 表名
    pub table: &'static str,
    /// 主键列名，默认 `id`
    pub id_col: &'static str,
    /// 树编码列名，默认 `tree_code`
    pub code_col: &'static str,
    /// 外层隔离列名；`None` 表示全局一棵树
    pub scope_col: Option<&'static str>,
    /// 操作人列名，默认 `updater_id`
    pub updater_col: &'static str,
    /// 更新时间列名，默认 `update_ms`
    pub update_ms_col: &'static str,
    /// 每层段值步长，默认 [`DEFAULT_TREE_STEP`]
    pub step: usize,
    /// 编码最大总长度，默认 [`DEFAULT_TREE_MAX_LENGTH`]
    pub max_length: usize,
    /// 同一父节点最大兄弟数，默认 [`DEFAULT_TREE_MAX_SIBLINGS`]
    pub max_siblings: usize,
}

impl TreeSort {
    /// 用默认列名构造表描述
    pub const fn new(table: &'static str) -> Self {
        Self {
            table,
            id_col: "id",
            code_col: "tree_code",
            scope_col: None,
            updater_col: "updater_id",
            update_ms_col: "update_ms",
            step: DEFAULT_TREE_STEP,
            max_length: DEFAULT_TREE_MAX_LENGTH,
            max_siblings: DEFAULT_TREE_MAX_SIBLINGS,
        }
    }

    /// 指定外层隔离列
    pub const fn with_scope(mut self, col: &'static str) -> Self {
        self.scope_col = Some(col);
        self
    }

    /// 指定编码最大长度与同父最大兄弟数
    pub const fn with_limits(mut self, max_length: usize, max_siblings: usize) -> Self {
        self.max_length = max_length;
        self.max_siblings = max_siblings;
        self
    }
}

fn to_svc_error(error: DbErr) -> SvcError {
    SvcError::from(DaoError::parse_db_err(error))
}

fn invalid(message: impl Into<String>) -> SvcError {
    SvcError::Runtime(anyhow!(message.into()))
}

/// 分组 advisory lock 的 key
fn group_key(table: &str, group_col: Option<&str>, group_value: Option<i64>) -> String {
    match (group_col, group_value) {
        (None, _) => format!("{table}:*"),
        (Some(col), Some(value)) => format!("{table}:{col}={value}"),
        (Some(col), None) => format!("{table}:{col}=null"),
    }
}

/// 生成「组条件」SQL 与参数；`start` 是下一个可用占位符序号。
fn group_clause(
    group_col: Option<&str>,
    group_value: Option<i64>,
    start: usize,
) -> (String, Vec<Value>) {
    match group_col {
        None => ("TRUE".to_string(), Vec::new()),
        Some(col) => match group_value {
            Some(_) => (format!("{col} = ${start}"), vec![Value::BigInt(group_value)]),
            None => (format!("{col} IS NULL"), Vec::new()),
        },
    }
}

async fn query_all<C>(db: &C, sql: &str, values: Vec<Value>) -> Result<Vec<QueryResult>, SvcError>
where
    C: ConnectionTrait,
{
    db.query_all_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        values,
    ))
    .await
    .map_err(to_svc_error)
}

async fn execute<C>(db: &C, sql: &str, values: Vec<Value>) -> Result<u64, SvcError>
where
    C: ConnectionTrait,
{
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        values,
    ))
    .await
    .map(|result| result.rows_affected())
    .map_err(to_svc_error)
}

/// 事务级 advisory lock：同一 key 的并发事务在这里排队；同一事务重复加锁可重入。
async fn lock_key<C>(db: &C, key: &str) -> Result<(), SvcError>
where
    C: ConnectionTrait,
{
    execute(
        db,
        "SELECT pg_advisory_xact_lock(hashtext($1)::bigint)",
        vec![Value::String(Some(format!("robotech_sort:{key}")))],
    )
    .await?;
    Ok(())
}

// ==================== 平铺实体 ====================

/// # 取同组下一个排序值
///
/// 在调用方事务内先锁分组，再取 `max(sort_order)`；空组从 `1` 开始。
/// 返回值超过 [`FlatSort::max`] 时返回错误，调用方应先重排整理。
pub async fn next_sort_order<C>(
    db: &C,
    spec: &FlatSort,
    group_value: Option<i64>,
) -> Result<i32, SvcError>
where
    C: ConnectionTrait + Sync,
{
    lock_key(db, &group_key(spec.table, spec.group_col, group_value)).await?;

    let (clause, mut values) = group_clause(spec.group_col, group_value, 1);
    values.shrink_to_fit();
    let sql = format!(
        "SELECT COALESCE(MAX({sort_col}), 0) AS max_sort FROM {table} WHERE {clause}",
        sort_col = spec.sort_col,
        table = spec.table,
    );
    let rows = query_all(db, &sql, values).await?;
    let max = rows
        .first()
        .ok_or_else(|| invalid("读取排序最大值失败"))?
        .try_get::<i32>("", "max_sort")
        .map_err(to_svc_error)?;
    let next = max.saturating_add(1);
    if next > spec.max {
        return Err(invalid(format!(
            "该分组排序值已达上限 {}，请使用重排接口整理",
            spec.max
        )));
    }
    Ok(next)
}

fn validate_ids(ids: &[u64]) -> Result<(), SvcError> {
    if ids.is_empty() {
        return Err(invalid("排序列表不能为空"));
    }
    let unique = ids.iter().copied().collect::<HashSet<_>>();
    if unique.len() != ids.len() {
        return Err(invalid("排序列表存在重复 ID"));
    }
    Ok(())
}

fn ids_clause(ids: &[u64]) -> (String, Vec<Value>) {
    let placeholders = ids
        .iter()
        .enumerate()
        .map(|(index, _)| format!("${}", index + 1))
        .collect::<Vec<_>>()
        .join(",");
    let values = ids
        .iter()
        .map(|id| Value::BigInt(Some(*id as i64)))
        .collect();
    (placeholders, values)
}

/// # 平铺实体重排
///
/// [`FlatSort::group_col`] 为分组列；`None` 表示全表一组。
/// 提交的 id 必须恰好覆盖同组全部记录，服务端按提交顺序写回 `1..n`。
pub async fn reorder_flat<C>(
    db: &C,
    spec: &FlatSort,
    ids: &[u64],
    operator_id: Option<u64>,
) -> Result<u64, SvcError>
where
    C: ConnectionTrait + Sync,
{
    validate_ids(ids)?;
    reorder_flat_in_tx(db, spec, ids, operator_id).await
}

async fn reorder_flat_in_tx<C>(
    db: &C,
    spec: &FlatSort,
    ids: &[u64],
    operator_id: Option<u64>,
) -> Result<u64, SvcError>
where
    C: ConnectionTrait,
{
    let (placeholders, id_values) = ids_clause(ids);
    let select_sql = match spec.group_col {
        Some(col) => format!(
            "SELECT {id_col} AS id, {sort_col} AS sort_value, {col} AS group_value FROM {table} WHERE {id_col} IN ({placeholders})",
            id_col = spec.id_col,
            sort_col = spec.sort_col,
            table = spec.table,
        ),
        None => format!(
            "SELECT {id_col} AS id, {sort_col} AS sort_value, NULL::bigint AS group_value FROM {table} WHERE {id_col} IN ({placeholders})",
            id_col = spec.id_col,
            sort_col = spec.sort_col,
            table = spec.table,
        ),
    };
    let rows = query_all(db, &select_sql, id_values).await?;
    if rows.len() != ids.len() {
        return Err(invalid("排序列表包含不存在或不属于该实体的记录"));
    }

    let group_value = rows
        .first()
        .ok_or_else(|| invalid("排序列表不能为空"))?
        .try_get::<Option<i64>>("", "group_value")
        .map_err(to_svc_error)?;
    for row in &rows {
        let current = row
            .try_get::<Option<i64>>("", "group_value")
            .map_err(to_svc_error)?;
        if current != group_value {
            return Err(invalid("排序列表跨越了多个分组，请按同组记录提交"));
        }
    }

    lock_key(db, &group_key(spec.table, spec.group_col, group_value)).await?;

    let (clause, mut values) = group_clause(spec.group_col, group_value, 1);
    values.shrink_to_fit();
    let lock_sql = format!(
        "SELECT {id_col} AS id FROM {table} WHERE {clause} ORDER BY {sort_col}, {id_col} FOR UPDATE",
        id_col = spec.id_col,
        sort_col = spec.sort_col,
        table = spec.table,
    );
    let locked = query_all(db, &lock_sql, values).await?;
    let existing = locked
        .iter()
        .map(|row| row.try_get::<i64>("", "id").map(|id| id as u64))
        .collect::<Result<HashSet<_>, DbErr>>()
        .map_err(to_svc_error)?;
    if existing.len() != ids.len() || !ids.iter().all(|id| existing.contains(id)) {
        return Err(invalid(
            "排序列表必须恰好覆盖同一分组的全部记录（有并发新增或删除时请刷新后重试）",
        ));
    }

    let update_ms = now_ms();
    let mut affected = 0u64;
    for (index, id) in ids.iter().enumerate() {
        match operator_id {
            Some(operator_id) => {
                affected += execute(
                    db,
                    &format!(
                        "UPDATE {table} SET {sort_col} = $1, {updater_col} = $2, {update_ms_col} = $3 WHERE {id_col} = $4",
                        table = spec.table,
                        sort_col = spec.sort_col,
                        updater_col = spec.updater_col,
                        update_ms_col = spec.update_ms_col,
                        id_col = spec.id_col,
                    ),
                    vec![
                        Value::Int(Some(index as i32 + 1)),
                        Value::BigInt(Some(operator_id as i64)),
                        Value::BigInt(Some(update_ms as i64)),
                        Value::BigInt(Some(*id as i64)),
                    ],
                )
                .await?;
            }
            None => {
                affected += execute(
                    db,
                    &format!(
                        "UPDATE {table} SET {sort_col} = $1, {update_ms_col} = $2 WHERE {id_col} = $3",
                        table = spec.table,
                        sort_col = spec.sort_col,
                        update_ms_col = spec.update_ms_col,
                        id_col = spec.id_col,
                    ),
                    vec![
                        Value::Int(Some(index as i32 + 1)),
                        Value::BigInt(Some(update_ms as i64)),
                        Value::BigInt(Some(*id as i64)),
                    ],
                )
                .await?;
            }
        }
    }
    Ok(affected)
}

/// # 删除单条后压缩同组序号
///
/// 同一分组内 `sort_order > removed_sort_order` 的记录整体 −1，后面的序号前移，
/// 不留空洞。与重排一致：先拿分组 advisory lock，再 `FOR UPDATE` 锁住受影响的
/// 记录，最后一次性前移；都在调用方事务里，失败整体回滚。
pub async fn compact_flat_after_delete<C>(
    db: &C,
    spec: &FlatSort,
    group_value: Option<i64>,
    removed_sort_order: i32,
    operator_id: Option<u64>,
) -> Result<u64, SvcError>
where
    C: ConnectionTrait + Sync,
{
    lock_key(db, &group_key(spec.table, spec.group_col, group_value)).await?;

    let (clause, mut values) = group_clause(spec.group_col, group_value, 1);
    let removed_idx = values.len() + 1;
    values.push(Value::Int(Some(removed_sort_order)));

    let lock_sql = format!(
        "SELECT {id_col} AS id FROM {table} WHERE {clause} AND {sort_col} > ${removed_idx} ORDER BY {sort_col}, {id_col} FOR UPDATE",
        id_col = spec.id_col,
        sort_col = spec.sort_col,
        table = spec.table,
    );
    query_all(db, &lock_sql, values.clone()).await?;

    let now_idx = values.len() + 1;
    values.push(Value::BigInt(Some(now_ms() as i64)));
    let mut sets = format!(
        "{sort_col} = {sort_col} - 1, {update_ms_col} = ${now_idx}",
        sort_col = spec.sort_col,
        update_ms_col = spec.update_ms_col,
    );
    if let Some(operator_id) = operator_id {
        let op_idx = values.len() + 1;
        values.push(Value::BigInt(Some(operator_id as i64)));
        sets.push_str(&format!(", {updater_col} = ${op_idx}", updater_col = spec.updater_col));
    }
    execute(
        db,
        &format!(
            "UPDATE {table} SET {sets} WHERE {clause} AND {sort_col} > ${removed_idx}",
            table = spec.table,
            sort_col = spec.sort_col,
        ),
        values,
    )
    .await
}

/// # 整组重算连续序号（`1..n`）
///
/// 批量删除后调用：只对出现空洞的组做一次整组重算，不逐条压缩。
/// 组内按 `(sort_order, id)` 排序，写回 `1..n`。
async fn compact_flat_group_in_tx<C>(
    db: &C,
    spec: &FlatSort,
    group_value: Option<i64>,
    operator_id: Option<u64>,
) -> Result<u64, SvcError>
where
    C: ConnectionTrait,
{
    let (clause, values) = group_clause(spec.group_col, group_value, 1);
    let sql = format!(
        "SELECT {id_col} AS id, {sort_col} AS sort_value FROM {table} WHERE {clause} ORDER BY {sort_col}, {id_col} FOR UPDATE",
        id_col = spec.id_col,
        sort_col = spec.sort_col,
        table = spec.table,
    );
    let rows = query_all(db, &sql, values).await?;

    let update_ms = now_ms();
    let mut affected = 0u64;
    for (index, row) in rows.iter().enumerate() {
        let id = row.try_get::<i64>("", "id").map_err(to_svc_error)?;
        let current = row
            .try_get::<i32>("", "sort_value")
            .map_err(to_svc_error)?;
        let target = index as i32 + 1;
        if current == target {
            continue;
        }
        affected += match operator_id {
            Some(operator_id) => {
                execute(
                    db,
                    &format!(
                        "UPDATE {table} SET {sort_col} = $1, {updater_col} = $2, {update_ms_col} = $3 WHERE {id_col} = $4",
                        table = spec.table,
                        sort_col = spec.sort_col,
                        updater_col = spec.updater_col,
                        update_ms_col = spec.update_ms_col,
                        id_col = spec.id_col,
                    ),
                    vec![
                        Value::Int(Some(target)),
                        Value::BigInt(Some(operator_id as i64)),
                        Value::BigInt(Some(update_ms as i64)),
                        Value::BigInt(Some(id)),
                    ],
                )
                .await?
            }
            None => {
                execute(
                    db,
                    &format!(
                        "UPDATE {table} SET {sort_col} = $1, {update_ms_col} = $2 WHERE {id_col} = $3",
                        table = spec.table,
                        sort_col = spec.sort_col,
                        update_ms_col = spec.update_ms_col,
                        id_col = spec.id_col,
                    ),
                    vec![
                        Value::Int(Some(target)),
                        Value::BigInt(Some(update_ms as i64)),
                        Value::BigInt(Some(id)),
                    ],
                )
                .await?
            }
        };
    }
    Ok(affected)
}

/// # 批量删除后压缩平铺实体
///
/// 扫描全表所有分组，只对「最大值不等于条数」或「序号有重复」的组整组重算，
/// 每组重算一次；组之间按 key 字符串排序加锁，避免并发批量删除互相等待。
pub async fn compact_flat_gaps<C>(db: &C, spec: &FlatSort) -> Result<u64, SvcError>
where
    C: ConnectionTrait + Sync,
{
    let sort_col = spec.sort_col;
    let table = spec.table;
    let sql = match spec.group_col {
        Some(col) => format!(
            "SELECT {col} AS group_value, MAX({sort_col}) AS max_sort, COUNT(*) AS cnt,
                    COUNT(DISTINCT {sort_col}) AS distinct_cnt
             FROM {table}
             GROUP BY {col}
             HAVING MAX({sort_col}) <> COUNT(*) OR COUNT(DISTINCT {sort_col}) <> COUNT(*)"
        ),
        None => format!(
            "SELECT NULL::bigint AS group_value, MAX({sort_col}) AS max_sort, COUNT(*) AS cnt,
                    COUNT(DISTINCT {sort_col}) AS distinct_cnt
             FROM {table}
             HAVING MAX({sort_col}) <> COUNT(*) OR COUNT(DISTINCT {sort_col}) <> COUNT(*)"
        ),
    };
    let rows = query_all(db, &sql, Vec::new()).await?;
    let mut groups = rows
        .iter()
        .map(|row| {
            row.try_get::<Option<i64>>("", "group_value")
                .map_err(to_svc_error)
        })
        .collect::<Result<Vec<_>, SvcError>>()?;
    groups.sort_by_key(|value| group_key(table, spec.group_col, *value));

    let mut affected = 0u64;
    for group in groups {
        lock_key(db, &group_key(table, spec.group_col, group)).await?;
        affected += compact_flat_group_in_tx(db, spec, group, None).await?;
    }
    Ok(affected)
}

/// # 校验平铺分组内无同号冲突
///
/// `exclude_id` 用于「改分组」场景：此时被移动的记录还在旧分组里，且旧分组
/// 后续记录已经前移，两者会短暂同号；把它排除后校验的才是移动完成后的状态。
async fn assert_flat_group_unique<C>(
    db: &C,
    spec: &FlatSort,
    group_value: Option<i64>,
    exclude_id: Option<u64>,
) -> Result<(), SvcError>
where
    C: ConnectionTrait,
{
    let (mut clause, mut values) = group_clause(spec.group_col, group_value, 1);
    if let Some(id) = exclude_id {
        values.push(Value::BigInt(Some(id as i64)));
        clause.push_str(&format!(" AND {} <> ${}", spec.id_col, values.len()));
    }
    let sql = format!(
        "SELECT COUNT(*) AS cnt, COUNT(DISTINCT {sort_col}) AS distinct_cnt FROM {table} WHERE {clause}",
        sort_col = spec.sort_col,
        table = spec.table,
    );
    let rows = query_all(db, &sql, values).await?;
    let row = rows
        .first()
        .ok_or_else(|| invalid("校验排序结果失败"))?;
    let cnt = row.try_get::<i64>("", "cnt").map_err(to_svc_error)?;
    let distinct = row
        .try_get::<i64>("", "distinct_cnt")
        .map_err(to_svc_error)?;
    if cnt != distinct {
        return Err(invalid("分组内存在重复排序值，请刷新后重试"));
    }
    Ok(())
}

/// # 平铺实体改分组
///
/// 分组列变化时，新分组排到末尾（`max+1`），旧分组里排在它后面的记录整体前移 −1；
/// 两组在同一事务内完成。返回新排序值（`None` 表示分组没变或该表无分组列）。
///
/// 加锁顺序：先按组 key 字符串排序拿 advisory lock，再把受影响的记录
/// `FOR UPDATE` 锁住——A→B 与 B→A 并发时不会互相等待。
pub async fn flat_move_group<C>(
    db: &C,
    spec: &FlatSort,
    id: u64,
    new_group: Option<i64>,
    operator_id: Option<u64>,
) -> Result<Option<i32>, SvcError>
where
    C: ConnectionTrait + Sync,
{
    // 没有分组列时不存在「改分组」，直接视为没变化。
    let Some(group_col) = spec.group_col else {
        return Ok(None);
    };

    // 先无锁读分组、按 key 排序拿两把分组锁，最后才锁行。
    // 反过来「先锁行、再等分组锁」会与并发压缩形成行锁 ↔ 分组锁的环形等待：
    // 对方持有分组锁正在前移它后面的记录，恰好包含我们锁住的那一行。
    let rows = query_all(
        db,
        &format!(
            "SELECT {group_col} AS group_value FROM {table} WHERE {id_col} = $1",
            table = spec.table,
            id_col = spec.id_col,
        ),
        vec![Value::BigInt(Some(id as i64))],
    )
    .await?;
    let Some(row) = rows.first() else {
        // 记录不存在：交给框架的 update 去报 NotFound
        return Ok(None);
    };
    let old_group = row
        .try_get::<Option<i64>>("", "group_value")
        .map_err(to_svc_error)?;
    if old_group == new_group {
        return Ok(None);
    }

    let old_key = group_key(spec.table, Some(group_col), old_group);
    let new_key = group_key(spec.table, Some(group_col), new_group);
    if old_key <= new_key {
        lock_key(db, &old_key).await?;
        lock_key(db, &new_key).await?;
    } else {
        lock_key(db, &new_key).await?;
        lock_key(db, &old_key).await?;
    }

    // 拿到分组锁后重读：分组若已被并发改走，本次请求基于旧分组算出的前移
    // 已经没有意义，直接要求刷新，避免在错误的组上压缩。
    let rows = query_all(
        db,
        &format!(
            "SELECT {id_col} AS id, {sort_col} AS sort_value, {group_col} AS group_value FROM {table} WHERE {id_col} = $1 FOR UPDATE",
            id_col = spec.id_col,
            sort_col = spec.sort_col,
            table = spec.table,
        ),
        vec![Value::BigInt(Some(id as i64))],
    )
    .await?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let current_group = row
        .try_get::<Option<i64>>("", "group_value")
        .map_err(to_svc_error)?;
    if current_group != old_group {
        return Err(invalid("记录的分组已被并发修改，请刷新后重试"));
    }
    let old_sort = row
        .try_get::<i32>("", "sort_value")
        .map_err(to_svc_error)?;

    // 新分组末尾
    let next = next_sort_order(db, spec, new_group).await?;
    // 旧分组后续前移
    compact_flat_after_delete(db, spec, old_group, old_sort, operator_id).await?;

    assert_flat_group_unique(db, spec, old_group, Some(id)).await?;
    assert_flat_group_unique(db, spec, new_group, None).await?;
    Ok(Some(next))
}

// ==================== 树形实体 ====================

/// 取父编码：合法树编码是若干 3 位数字段，去掉最后一段即父；根编码长度为 3。
fn tree_parent(code: &str) -> Option<&str> {
    if code.len() < 3 || code.len() % 3 != 0 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(&code[..code.len() - 3])
}

/// 临时编码：`~` + base36 两位（最多 1296 个），不会与合法 `tree_code` 冲突。
fn base36_pair(index: usize) -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let high = ALPHABET[(index / 36) % 36] as char;
    let low = ALPHABET[index % 36] as char;
    format!("~{high}{low}")
}

/// # 树形实体重排（按树编码）
///
/// [`TreeSort::scope_col`] 是外层隔离列（菜单 `app_id`、角色/权限分组 `realm_id`）；
/// `None` 表示全局一棵树（如组织）。
///
/// 算法：
/// 1. 校验提交的 id 必须同属一个父节点、一个外层隔离域；
/// 2. 锁住同父全部兄弟后，按提交顺序生成连续子段 `001..n`；
/// 3. 先把每个兄弟子树挪到临时编码，再落到最终编码，避免唯一键冲突；
/// 4. 整棵树（含后代）在同一事务内级联更新。
pub async fn reorder_tree<C>(
    db: &C,
    spec: &TreeSort,
    ids: &[u64],
    operator_id: Option<u64>,
) -> Result<u64, SvcError>
where
    C: ConnectionTrait + Sync,
{
    validate_ids(ids)?;
    reorder_tree_in_tx(db, spec, ids, operator_id).await
}

async fn reorder_tree_in_tx<C>(
    db: &C,
    spec: &TreeSort,
    ids: &[u64],
    operator_id: Option<u64>,
) -> Result<u64, SvcError>
where
    C: ConnectionTrait,
{
    let (placeholders, id_values) = ids_clause(ids);
    let select_sql = match spec.scope_col {
        Some(col) => format!(
            "SELECT {id_col} AS id, {code_col} AS code_value, {col} AS scope_value FROM {table} WHERE {id_col} IN ({placeholders})",
            id_col = spec.id_col,
            code_col = spec.code_col,
            table = spec.table,
        ),
        None => format!(
            "SELECT {id_col} AS id, {code_col} AS code_value, NULL::bigint AS scope_value FROM {table} WHERE {id_col} IN ({placeholders})",
            id_col = spec.id_col,
            code_col = spec.code_col,
            table = spec.table,
        ),
    };
    let rows = query_all(db, &select_sql, id_values).await?;
    if rows.len() != ids.len() {
        return Err(invalid("排序列表包含不存在或不属于该实体的记录"));
    }

    let scope_value = rows
        .first()
        .ok_or_else(|| invalid("排序列表不能为空"))?
        .try_get::<Option<i64>>("", "scope_value")
        .map_err(to_svc_error)?;
    let first_code = rows
        .first()
        .ok_or_else(|| invalid("排序列表不能为空"))?
        .try_get::<String>("", "code_value")
        .map_err(to_svc_error)?;
    let parent = tree_parent(&first_code)
        .ok_or_else(|| invalid(format!("非法的 tree_code：{first_code}")))?
        .to_string();
    let sibling_len = parent.len() + 3;

    let mut submitted = HashMap::new();
    for row in &rows {
        let current_scope = row
            .try_get::<Option<i64>>("", "scope_value")
            .map_err(to_svc_error)?;
        let code = row
            .try_get::<String>("", "code_value")
            .map_err(to_svc_error)?;
        if current_scope != scope_value {
            return Err(invalid("排序列表跨越了多个隔离域，请按同一外层范围提交"));
        }
        if tree_parent(&code) != Some(parent.as_str()) {
            return Err(invalid("排序列表只能包含同一父节点的兄弟记录"));
        }
        let id = row.try_get::<i64>("", "id").map_err(to_svc_error)? as u64;
        submitted.insert(id, code);
    }

    // 树形重排会使用临时编码，按表串行化，避免不同父节点并发时临时编码互相干扰。
    lock_key(db, &format!("tree:{}", spec.table)).await?;

    let (scope_clause, values) = tree_scope_clause(spec.scope_col, scope_value, 1);
    let mut lock_sql = format!(
        "SELECT {id_col} AS id, {code_col} AS code_value FROM {table} WHERE {scope_clause}",
        id_col = spec.id_col,
        code_col = spec.code_col,
        table = spec.table,
    );
    let mut lock_values = values;
    // 占位符按实际追加顺序编号：根节点没有父编码参数，不能预留下 $2 的空位。
    if !parent.is_empty() {
        let parent_index = lock_values.len() + 1;
        lock_sql.push_str(&format!(
            " AND {code_col} LIKE ${parent_index} || '%'",
            code_col = spec.code_col,
        ));
        lock_values.push(Value::String(Some(parent.clone())));
    }
    let len_index = lock_values.len() + 1;
    lock_sql.push_str(&format!(
        " AND length({code_col}) = ${len_index}",
        code_col = spec.code_col,
    ));
    lock_values.push(Value::Int(Some(sibling_len as i32)));
    lock_sql.push_str(&format!(" ORDER BY {code_col} FOR UPDATE", code_col = spec.code_col));
    let locked = query_all(db, &lock_sql, lock_values).await?;

    if locked.len() != ids.len() {
        return Err(invalid(
            "排序列表必须恰好覆盖同一父节点的全部兄弟记录（有并发新增或删除时请刷新后重试）",
        ));
    }
    for row in &locked {
        let id = row.try_get::<i64>("", "id").map_err(to_svc_error)? as u64;
        let code = row
            .try_get::<String>("", "code_value")
            .map_err(to_svc_error)?;
        if submitted.get(&id) != Some(&code) {
            return Err(invalid("排序列表与当前树结构不一致，请刷新后重试"));
        }
    }
    if ids.len() > spec.max_siblings {
        return Err(invalid(format!(
            "同一父节点的兄弟记录超过 {} 条，无法生成三位子段",
            spec.max_siblings
        )));
    }

    let codes = ids
        .iter()
        .map(|id| {
            submitted
                .get(id)
                .cloned()
                .ok_or_else(|| invalid("排序列表与当前树结构不一致，请刷新后重试"))
        })
        .collect::<Result<Vec<_>, SvcError>>()?;

    // 复用旧编码前检查孤儿：重排会把旧槽位上的孤儿后代挂到新兄弟下。
    let sibling_set: HashSet<&str> = codes.iter().map(|code| code.as_str()).collect();
    if has_orphan_descendants(db, spec, scope_value, &parent, &sibling_set).await? {
        return Err(invalid(
            "该父节点下存在孤立的子节点（历史删除残留），请先清理后再重排",
        ));
    }

    rewrite_siblings(db, spec, scope_value, &parent, &codes, operator_id).await
}

/// # 把一组兄弟按给定顺序改写成连续子段
///
/// `codes` 是**当前**编码、按目标顺序排列；`parent` 是它们的父编码前缀。
///
/// 分两段写：
/// 1. 每个兄弟（含整棵子树）先挪到临时编码 `~xx`，避开树编码唯一约束
///    （直接相邻互换时两个编码会同时占用对方的位置）；
/// 2. 再按 `parent + 001..n` 落位，后代用 `substr` 保留相对段。
///
/// 调用方需先持有表级 advisory lock（`tree:{table}`）——临时编码只在事务内可见，
/// 但仍要避免同表不同父节点的两次重排互相踩到对方的临时编码。
async fn rewrite_siblings<C>(
    db: &C,
    spec: &TreeSort,
    scope_value: Option<i64>,
    parent: &str,
    codes: &[String],
    operator_id: Option<u64>,
) -> Result<u64, SvcError>
where
    C: ConnectionTrait,
{
    if codes.len() > spec.max_siblings {
        return Err(invalid(format!(
            "同一父节点的兄弟记录超过 {} 条，无法生成三位子段",
            spec.max_siblings
        )));
    }
    if codes.len() > TEMP_CODE_CAPACITY {
        return Err(invalid(format!(
            "同一父节点的兄弟记录超过 {TEMP_CODE_CAPACITY} 条，临时编码容量不足，无法重排"
        )));
    }

    let table = spec.table;
    let code_col = spec.code_col;
    let update_ms = now_ms();
    let mut affected = 0u64;

    // 第一段：按目标顺序把每个兄弟子树挪到临时编码（`~` 开头，不会与合法 tree_code 冲突）
    for (index, code) in codes.iter().enumerate() {
        let temp = base36_pair(index);
        let (sql, values) = match operator_id {
            Some(operator_id) => {
                let (scope_clause, scope_values) =
                    tree_scope_clause(spec.scope_col, scope_value, 7);
                let mut values = vec![
                    Value::String(Some(temp)),
                    Value::Int(Some((parent.len() + 3) as i32)),
                    Value::Int(Some((parent.len() + 4) as i32)),
                    Value::BigInt(Some(operator_id as i64)),
                    Value::BigInt(Some(update_ms as i64)),
                    Value::String(Some(code.clone())),
                ];
                values.extend(scope_values);
                (
                    format!(
                        "UPDATE {table}
                         SET {code_col} = $1 || CASE WHEN length({code_col}) > $2 THEN substr({code_col}, $3) ELSE '' END,
                             {updater_col} = $4, {update_ms_col} = $5
                         WHERE {code_col} LIKE $6 || '%' AND {scope_clause}",
                        table = table,
                        code_col = code_col,
                        updater_col = spec.updater_col,
                        update_ms_col = spec.update_ms_col,
                    ),
                    values,
                )
            }
            None => {
                let (scope_clause, scope_values) =
                    tree_scope_clause(spec.scope_col, scope_value, 6);
                let mut values = vec![
                    Value::String(Some(temp)),
                    Value::Int(Some((parent.len() + 3) as i32)),
                    Value::Int(Some((parent.len() + 4) as i32)),
                    Value::BigInt(Some(update_ms as i64)),
                    Value::String(Some(code.clone())),
                ];
                values.extend(scope_values);
                (
                    format!(
                        "UPDATE {table}
                         SET {code_col} = $1 || CASE WHEN length({code_col}) > $2 THEN substr({code_col}, $3) ELSE '' END,
                             {update_ms_col} = $4
                         WHERE {code_col} LIKE $5 || '%' AND {scope_clause}",
                        table = table,
                        code_col = code_col,
                        update_ms_col = spec.update_ms_col,
                    ),
                    values,
                )
            }
        };
        affected += execute(db, &sql, values).await?;
    }

    // 第二段：按目标顺序落到最终编码（补零到 3 位，1..n 连续）
    for index in 0..codes.len() {
        let final_code = format!("{parent}{:03}", index + 1);
        if final_code.len() > spec.max_length {
            return Err(invalid(format!(
                "tree_code 超过 {} 位，无法重排",
                spec.max_length
            )));
        }
        let temp = base36_pair(index);
        let (sql, values) = match operator_id {
            Some(operator_id) => {
                let scope_values = match (spec.scope_col, scope_value) {
                    (Some(_), Some(_)) => vec![Value::BigInt(scope_value)],
                    _ => Vec::new(),
                };
                let sql = match (spec.scope_col, scope_value) {
                    (Some(col), Some(_)) => format!(
                        "UPDATE {table}
                         SET {code_col} = $1 || substr({code_col}, 4), {updater_col} = $2, {update_ms_col} = $3
                         WHERE {code_col} LIKE $4 || '%' AND {col} = $5",
                        table = table,
                        code_col = code_col,
                        updater_col = spec.updater_col,
                        update_ms_col = spec.update_ms_col,
                    ),
                    (Some(col), None) => format!(
                        "UPDATE {table}
                         SET {code_col} = $1 || substr({code_col}, 4), {updater_col} = $2, {update_ms_col} = $3
                         WHERE {code_col} LIKE $4 || '%' AND {col} IS NULL",
                        table = table,
                        code_col = code_col,
                        updater_col = spec.updater_col,
                        update_ms_col = spec.update_ms_col,
                    ),
                    (None, _) => format!(
                        "UPDATE {table}
                         SET {code_col} = $1 || substr({code_col}, 4), {updater_col} = $2, {update_ms_col} = $3
                         WHERE {code_col} LIKE $4 || '%'",
                        table = table,
                        code_col = code_col,
                        updater_col = spec.updater_col,
                        update_ms_col = spec.update_ms_col,
                    ),
                };
                let mut values = vec![
                    Value::String(Some(final_code)),
                    Value::BigInt(Some(operator_id as i64)),
                    Value::BigInt(Some(update_ms as i64)),
                    Value::String(Some(temp)),
                ];
                values.extend(scope_values);
                (sql, values)
            }
            None => {
                let scope_values = match (spec.scope_col, scope_value) {
                    (Some(_), Some(_)) => vec![Value::BigInt(scope_value)],
                    _ => Vec::new(),
                };
                let sql = match (spec.scope_col, scope_value) {
                    (Some(col), Some(_)) => format!(
                        "UPDATE {table}
                         SET {code_col} = $1 || substr({code_col}, 4), {update_ms_col} = $2
                         WHERE {code_col} LIKE $3 || '%' AND {col} = $4",
                        table = table,
                        code_col = code_col,
                        update_ms_col = spec.update_ms_col,
                    ),
                    (Some(col), None) => format!(
                        "UPDATE {table}
                         SET {code_col} = $1 || substr({code_col}, 4), {update_ms_col} = $2
                         WHERE {code_col} LIKE $3 || '%' AND {col} IS NULL",
                        table = table,
                        code_col = code_col,
                        update_ms_col = spec.update_ms_col,
                    ),
                    (None, _) => format!(
                        "UPDATE {table}
                         SET {code_col} = $1 || substr({code_col}, 4), {update_ms_col} = $2
                         WHERE {code_col} LIKE $3 || '%'",
                        table = table,
                        code_col = code_col,
                        update_ms_col = spec.update_ms_col,
                    ),
                };
                let values = {
                    let mut values = vec![
                        Value::String(Some(final_code)),
                        Value::BigInt(Some(update_ms as i64)),
                        Value::String(Some(temp)),
                    ];
                    values.extend(scope_values);
                    values
                };
                (sql, values)
            }
        };
        affected += execute(db, &sql, values).await?;
    }

    Ok(affected)
}

/// # 外层隔离条件 SQL
///
/// `start` 是下一个可用占位符序号，返回 `(子句, 参数)`；`TRUE` 表示不做隔离。
fn tree_scope_clause(
    scope_col: Option<&str>,
    scope_value: Option<i64>,
    start: usize,
) -> (String, Vec<Value>) {
    match (scope_col, scope_value) {
        (None, _) => ("TRUE".to_string(), Vec::new()),
        (Some(col), Some(_)) => (
            format!("{col} = ${start}"),
            vec![Value::BigInt(scope_value)],
        ),
        (Some(col), None) => (format!("{col} IS NULL"), Vec::new()),
    }
}

/// # 树表级 advisory lock
///
/// 树形实体的写操作统一按表串行化：临时编码、子树前缀级联都要求同一时刻
/// 只有一个人在改这张表的树结构。
pub async fn tree_lock<C>(db: &C, spec: &TreeSort) -> Result<(), SvcError>
where
    C: ConnectionTrait + Sync,
{
    lock_key(db, &format!("tree:{}", spec.table)).await
}

/// # 生成父节点下的下一个子编码
///
/// 封装 [`next_tree_code`]：按 [`TreeSort::scope_col`] 隔离、用
/// [`TreeSort::code_col`] 作为树编码列、按 [`TreeSort::step`] 步长生成。
/// 调用方负责先 [`tree_lock`] 拿表级锁，避免并发生成重复编码。
pub async fn next_child_code<C>(
    db: &C,
    spec: &TreeSort,
    scope: Option<i64>,
    parent: &str,
) -> Result<String, SvcError>
where
    C: ConnectionTrait + Sync,
{
    let scope_values: Vec<(&str, Option<Value>)> = match spec.scope_col {
        Some(col) => vec![(col, scope.map(|value| Value::BigInt(Some(value))))],
        None => Vec::new(),
    };
    let code = next_tree_code(
        db,
        spec.table,
        spec.code_col,
        parent,
        &scope_values,
        spec.step as u32,
    )
    .await?;
    if code.len() > spec.max_length {
        return Err(invalid(format!(
            "tree_code 超过 {} 位，无法在父节点 {parent:?} 下生成新编码",
            spec.max_length
        )));
    }
    Ok(code)
}

/// # 删除单条后压缩树形兄弟组
///
/// 被删节点所在父节点下、排在它后面的兄弟整体前移一位（含各自子树）。
/// 被删节点若仍有后代（路径枚举允许留下孤儿），跳过压缩，避免复用旧编码
/// 把孤儿挂到新的兄弟节点下。
pub async fn compact_tree_after_delete<C>(
    db: &C,
    spec: &TreeSort,
    scope: Option<i64>,
    tree_code: &str,
    operator_id: Option<u64>,
) -> Result<u64, SvcError>
where
    C: ConnectionTrait + Sync,
{
    let parent = tree_parent(tree_code)
        .ok_or_else(|| invalid(format!("非法的 tree_code：{tree_code}")))?
        .to_string();
    compact_tree_group_in_tx(db, spec, scope, &parent, operator_id).await
}

/// # 子树节点（含自身）
#[derive(Debug, Clone)]
pub struct TreeSubtreeNode {
    /// 主键
    pub id: u64,
    /// 树编码
    pub code: String,
}

/// # 读取节点及其全部后代
///
/// 按路径枚举前缀 + 外层隔离列取整棵子树（含节点自身），结果按编码排序
/// （字典序即前序：父节点紧跟子树）。节点不存在返回 `None`。
///
/// 只读，不加锁。需要「读到的子树」和后续写一致时，调用方应在同一事务内
/// 调用，并先 [`tree_lock`]；删除级联由 [`delete_tree_descendants`] 负责。
pub async fn tree_subtree<C>(
    db: &C,
    spec: &TreeSort,
    id: u64,
) -> Result<Option<Vec<TreeSubtreeNode>>, SvcError>
where
    C: ConnectionTrait + Sync,
{
    let select_sql = match spec.scope_col {
        Some(col) => format!(
            "SELECT {code_col} AS code_value, {col} AS scope_value FROM {table} WHERE {id_col} = $1",
            code_col = spec.code_col,
            table = spec.table,
            id_col = spec.id_col,
        ),
        None => format!(
            "SELECT {code_col} AS code_value, NULL::bigint AS scope_value FROM {table} WHERE {id_col} = $1",
            code_col = spec.code_col,
            table = spec.table,
            id_col = spec.id_col,
        ),
    };
    let rows = query_all(db, &select_sql, vec![Value::BigInt(Some(id as i64))]).await?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let code = row
        .try_get::<String>("", "code_value")
        .map_err(to_svc_error)?;
    let scope = row
        .try_get::<Option<i64>>("", "scope_value")
        .map_err(to_svc_error)?;

    let mut values = vec![Value::String(Some(code.clone()))];
    let (clause, scope_values) = tree_scope_clause(spec.scope_col, scope, 2);
    values.extend(scope_values);
    let sql = format!(
        "SELECT {id_col} AS id, {code_col} AS code_value FROM {table}
         WHERE {code_col} LIKE $1 || '%' AND {clause}
         ORDER BY {code_col}, {id_col}",
        id_col = spec.id_col,
        code_col = spec.code_col,
        table = spec.table,
    );
    let mut subtree = Vec::new();
    for row in query_all(db, &sql, values).await? {
        subtree.push(TreeSubtreeNode {
            id: row.try_get::<i64>("", "id").map_err(to_svc_error)? as u64,
            code: row
                .try_get::<String>("", "code_value")
                .map_err(to_svc_error)?,
        });
    }
    Ok(Some(subtree))
}

/// # 删除节点的全部子孙（不含节点自身）
///
/// 在调用方事务内先 [`tree_lock`]，锁住节点行后按「编码前缀 + 外层隔离列」
/// 删除全部后代，返回删除条数。节点本身由调用方（`del_by_id`）删除，随后
/// [`compact_tree_after_delete`] 负责同父兄弟前移；整组操作必须处于同一事务，
/// 任一失败整体回滚。
///
/// 业务层的引用保护（如角色绑定了菜单）应在调用本函数前完成校验并抛错，
/// 本函数只负责通用树结构，不感知任何业务关联。
pub async fn delete_tree_descendants<C>(
    db: &C,
    spec: &TreeSort,
    id: u64,
    _operator_id: Option<u64>,
) -> Result<u64, SvcError>
where
    C: ConnectionTrait + Sync,
{
    tree_lock(db, spec).await?;

    let select_sql = match spec.scope_col {
        Some(col) => format!(
            "SELECT {code_col} AS code_value, {col} AS scope_value FROM {table} WHERE {id_col} = $1 FOR UPDATE",
            code_col = spec.code_col,
            table = spec.table,
            id_col = spec.id_col,
        ),
        None => format!(
            "SELECT {code_col} AS code_value, NULL::bigint AS scope_value FROM {table} WHERE {id_col} = $1 FOR UPDATE",
            code_col = spec.code_col,
            table = spec.table,
            id_col = spec.id_col,
        ),
    };
    let rows = query_all(db, &select_sql, vec![Value::BigInt(Some(id as i64))]).await?;
    let Some(row) = rows.first() else {
        return Ok(0);
    };
    let code = row
        .try_get::<String>("", "code_value")
        .map_err(to_svc_error)?;
    let scope = row
        .try_get::<Option<i64>>("", "scope_value")
        .map_err(to_svc_error)?;

    let mut values = vec![
        Value::String(Some(code.clone())),
        Value::Int(Some(code.len() as i32)),
    ];
    let (clause, scope_values) = tree_scope_clause(spec.scope_col, scope, 3);
    values.extend(scope_values);
    let sql = format!(
        "DELETE FROM {table}
         WHERE {code_col} LIKE $1 || '%' AND length({code_col}) > $2 AND {clause}",
        table = spec.table,
        code_col = spec.code_col,
    );
    execute(db, &sql, values).await
}

/// # 把某个父节点下的兄弟重排成连续段
///
/// 已经连续时直接返回，不做写入；检测到「新槽位下面还有孤儿后代」时也跳过。
async fn compact_tree_group_in_tx<C>(
    db: &C,
    spec: &TreeSort,
    scope_value: Option<i64>,
    parent: &str,
    operator_id: Option<u64>,
) -> Result<u64, SvcError>
where
    C: ConnectionTrait,
{
    tree_lock(db, spec).await?;

    let table = spec.table;
    let code_col = spec.code_col;
    let (clause, mut values) = tree_scope_clause(spec.scope_col, scope_value, 1);
    let len_idx = values.len() + 1;
    values.push(Value::Int(Some((parent.len() + 3) as i32)));
    let mut sql = format!(
        "SELECT {id_col} AS id, {code_col} AS code_value FROM {table} WHERE {clause} AND length({code_col}) = ${len_idx}",
        id_col = spec.id_col,
    );
    if !parent.is_empty() {
        let parent_idx = values.len() + 1;
        sql.push_str(&format!(" AND {code_col} LIKE ${parent_idx} || '%'"));
        values.push(Value::String(Some(parent.to_string())));
    }
    sql.push_str(&format!(" ORDER BY {code_col}, {id_col} FOR UPDATE", id_col = spec.id_col));
    let rows = query_all(db, &sql, values).await?;

    let existing = rows
        .iter()
        .map(|row| row.try_get::<String>("", "code_value"))
        .collect::<Result<Vec<_>, DbErr>>()
        .map_err(to_svc_error)?;
    if existing.is_empty() {
        return Ok(0);
    }
    let contiguous = existing
        .iter()
        .enumerate()
        .all(|(index, code)| code == &format!("{parent}{:03}", index + 1));
    if contiguous {
        return Ok(0);
    }

    // 新编码若还压着「已删除节点的后代」（旧编码作为子树根），前移会把它挂到
    // 新的兄弟节点下。这种组保留空洞，不压缩。
    let existing_set: HashSet<&str> = existing.iter().map(|code| code.as_str()).collect();
    if has_orphan_descendants(db, spec, scope_value, parent, &existing_set).await? {
        return Ok(0);
    }

    rewrite_siblings(db, spec, scope_value, parent, &existing, operator_id).await
}

/// # 判断某个父节点下是否存在「根不在当前兄弟集合里」的后代
///
/// 路径枚举的删除不强制自引用外键，历史数据可能留下 `001001`（父 `001`
/// 已删）这类孤儿。压缩或重排复用旧编码时会把孤儿挂到新兄弟下，因此两类
/// 写操作都要先做这项检查：
/// - 压缩：发现孤儿就保留空洞，不压缩；
/// - 重排：直接报错，要求先清理孤儿。
async fn has_orphan_descendants<C>(
    db: &C,
    spec: &TreeSort,
    scope_value: Option<i64>,
    parent: &str,
    siblings: &HashSet<&str>,
) -> Result<bool, SvcError>
where
    C: ConnectionTrait,
{
    let table = spec.table;
    let code_col = spec.code_col;
    let mut values = vec![Value::Int(Some((parent.len() + 3) as i32))];
    let (orphan_clause, orphan_scope_values) =
        tree_scope_clause(spec.scope_col, scope_value, 2);
    let mut orphan_sql = format!(
        "SELECT substr({code_col}, 1, $1) AS root FROM {table}
         WHERE length({code_col}) > $1 AND {orphan_clause}",
    );
    values.extend(orphan_scope_values);
    if !parent.is_empty() {
        let parent_idx = values.len() + 1;
        orphan_sql.push_str(&format!(" AND {code_col} LIKE ${parent_idx} || '%'"));
        values.push(Value::String(Some(parent.to_string())));
    }
    orphan_sql.push_str(" GROUP BY 1");
    for row in query_all(db, &orphan_sql, values).await? {
        let root = row.try_get::<String>("", "root").map_err(to_svc_error)?;
        if !siblings.contains(root.as_str()) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// # 批量删除后压缩树形实体
///
/// 扫描所有「外层范围 + 父节点」分组，只对子段不连续的组各做一次整组重算。
/// 树表整体按 `tree:{table}` 串行化，组之间不需要再单独排序加锁。
pub async fn compact_tree_gaps<C>(db: &C, spec: &TreeSort) -> Result<u64, SvcError>
where
    C: ConnectionTrait + Sync,
{
    tree_lock(db, spec).await?;

    let table = spec.table;
    let code_col = spec.code_col;
    let sql = match spec.scope_col {
        Some(col) => format!(
            "SELECT {col} AS scope_value, substr({code_col}, 1, length({code_col}) - 3) AS parent
             FROM {table}
             GROUP BY {col}, 2
             HAVING MAX(substr({code_col}, length({code_col}) - 2))::int <> COUNT(*)
                 OR COUNT(DISTINCT substr({code_col}, length({code_col}) - 2)) <> COUNT(*)"
        ),
        None => format!(
            "SELECT NULL::bigint AS scope_value, substr({code_col}, 1, length({code_col}) - 3) AS parent
             FROM {table}
             GROUP BY 2
             HAVING MAX(substr({code_col}, length({code_col}) - 2))::int <> COUNT(*)
                 OR COUNT(DISTINCT substr({code_col}, length({code_col}) - 2)) <> COUNT(*)"
        ),
    };
    let rows = query_all(db, &sql, Vec::new()).await?;
    let mut groups = Vec::new();
    for row in &rows {
        let scope = row
            .try_get::<Option<i64>>("", "scope_value")
            .map_err(to_svc_error)?;
        let parent = row
            .try_get::<String>("", "parent")
            .map_err(to_svc_error)?;
        groups.push((scope, parent));
    }
    groups.sort_by(|left, right| {
        format!("{table}:{:?}:{}", left.0, left.1)
            .cmp(&format!("{table}:{:?}:{}", right.0, right.1))
    });

    let mut affected = 0u64;
    for (scope, parent) in groups {
        affected += compact_tree_group_in_tx(db, spec, scope, &parent, None).await?;
    }
    Ok(affected)
}

/// # 校验树形兄弟组内无同号冲突
async fn assert_tree_group_unique<C>(
    db: &C,
    spec: &TreeSort,
    scope_value: Option<i64>,
    parent: &str,
) -> Result<(), SvcError>
where
    C: ConnectionTrait,
{
    let code_col = spec.code_col;
    let (clause, mut values) = tree_scope_clause(spec.scope_col, scope_value, 1);
    let len_idx = values.len() + 1;
    values.push(Value::Int(Some((parent.len() + 3) as i32)));
    let mut sql = format!(
        "SELECT COUNT(*) AS cnt, COUNT(DISTINCT {code_col}) AS distinct_cnt
         FROM {table} WHERE {clause} AND length({code_col}) = ${len_idx}",
        table = spec.table,
    );
    if !parent.is_empty() {
        let parent_idx = values.len() + 1;
        sql.push_str(&format!(" AND {code_col} LIKE ${parent_idx} || '%'"));
        values.push(Value::String(Some(parent.to_string())));
    }
    let rows = query_all(db, &sql, values).await?;
    let row = rows
        .first()
        .ok_or_else(|| invalid("校验排序结果失败"))?;
    let cnt = row.try_get::<i64>("", "cnt").map_err(to_svc_error)?;
    let distinct = row
        .try_get::<i64>("", "distinct_cnt")
        .map_err(to_svc_error)?;
    if cnt != distinct {
        return Err(invalid("兄弟节点存在重复 tree_code，请刷新后重试"));
    }
    Ok(())
}

/// # 树形节点移动参数
pub struct TreeMoveArgs<'a> {
    /// 表描述
    pub spec: &'a TreeSort,
    /// 节点 ID
    pub id: u64,
    /// `None` 表示不改外层范围；`Some(v)` 表示改到 `v`
    pub new_scope: Option<i64>,
    /// `None` 表示不改父节点；`Some("")` 表示移到根
    pub new_parent: Option<String>,
    /// 操作人（写 `updater_id`），删除钩子场景可为 `None`
    pub operator_id: Option<u64>,
}

/// # 树形节点改父节点 / 改外层范围
///
/// 与 [`flat_move_group`] 等价，只是「分组」换成了「(外层范围, 父编码)」：
/// 新父下追加末位（[`next_child_code`] 即 `max+1`），旧父下排在它后面的兄弟前移 −1，
/// 两组在同一事务内完成，节点整棵子树级联改编码。
///
/// 返回 `true` 表示确实移动了；`false` 表示坐标没变或记录不存在（交给框架报 NotFound）。
pub async fn tree_move_node<C>(db: &C, args: TreeMoveArgs<'_>) -> Result<bool, SvcError>
where
    C: ConnectionTrait + Sync,
{
    if args.new_scope.is_none() && args.new_parent.is_none() {
        return Ok(false);
    }

    let spec = args.spec;
    let table = spec.table;
    let code_col = spec.code_col;
    tree_lock(db, spec).await?;

    let select_sql = match spec.scope_col {
        Some(col) => format!(
            "SELECT {code_col} AS code_value, {col} AS scope_value FROM {table} WHERE {id_col} = $1 FOR UPDATE",
            id_col = spec.id_col,
            table = table,
            code_col = code_col,
        ),
        None => format!(
            "SELECT {code_col} AS code_value, NULL::bigint AS scope_value FROM {table} WHERE {id_col} = $1 FOR UPDATE",
            id_col = spec.id_col,
            table = table,
            code_col = code_col,
        ),
    };
    let rows = query_all(db, &select_sql, vec![Value::BigInt(Some(args.id as i64))]).await?;
    let Some(row) = rows.first() else {
        return Ok(false);
    };
    let old_code = row
        .try_get::<String>("", "code_value")
        .map_err(to_svc_error)?;
    let old_scope = row
        .try_get::<Option<i64>>("", "scope_value")
        .map_err(to_svc_error)?;
    let old_parent = tree_parent(&old_code)
        .ok_or_else(|| invalid(format!("非法的 tree_code：{old_code}")))?
        .to_string();

    let target_scope = args.new_scope.or(old_scope);
    let scope_changed = args.new_scope.is_some() && target_scope != old_scope;
    let target_parent = args
        .new_parent
        .clone()
        .unwrap_or_else(|| old_parent.clone());
    let parent_changed = target_parent != old_parent;
    if !scope_changed && !parent_changed {
        return Ok(false);
    }

    // 防环：目标父不能是自己或自己的后代（编码前缀相同即后代）。
    // 编码只在同一外层隔离域内唯一；跨 scope 时两边的编码没有可比性，
    // 不能拿旧 scope 的编码前缀去判定新 scope 的父节点。
    if !target_parent.is_empty() {
        if !scope_changed && target_parent.starts_with(&old_code) {
            return Err(invalid("不能把节点移动到它自己的子树下"));
        }
        let mut values = vec![Value::String(Some(target_parent.clone()))];
        let mut sql = format!("SELECT 1 AS found FROM {table} WHERE {code_col} = $1");
        match (spec.scope_col, target_scope) {
            (Some(col), Some(value)) => {
                values.push(Value::BigInt(Some(value)));
                sql.push_str(&format!(" AND {col} = ${}", values.len()));
            }
            (Some(col), None) => sql.push_str(&format!(" AND {col} IS NULL")),
            (None, _) => {}
        }
        if query_all(db, &sql, values).await?.is_empty() {
            return Err(invalid("目标父节点不存在"));
        }
    }

    // 新父下的下一个可用编码（max+1）
    let new_code = next_child_code(db, spec, target_scope, &target_parent).await?;

    // 节点整棵子树级联改前缀（目标父编码 + 原相对段）；改外层范围时一并写回
    let mut values: Vec<Value> = vec![
        Value::String(Some(new_code.clone())),
        Value::Int(Some((old_code.len() + 1) as i32)),
    ];
    let mut sets = vec![format!("{code_col} = $1 || substr({code_col}, $2)")];
    if let Some(col) = spec.scope_col {
        match target_scope {
            Some(value) => {
                values.push(Value::BigInt(Some(value)));
                sets.push(format!("{col} = ${}", values.len()));
            }
            None => sets.push(format!("{col} = NULL")),
        }
    }
    values.push(Value::BigInt(Some(now_ms() as i64)));
    sets.push(format!(
        "{update_ms_col} = ${}",
        values.len(),
        update_ms_col = spec.update_ms_col
    ));
    if let Some(operator_id) = args.operator_id {
        values.push(Value::BigInt(Some(operator_id as i64)));
        sets.push(format!(
            "{updater_col} = ${}",
            values.len(),
            updater_col = spec.updater_col
        ));
    }
    values.push(Value::String(Some(old_code.clone())));
    let code_idx = values.len();
    let mut where_clause = format!("{code_col} LIKE ${code_idx} || '%'");
    match (spec.scope_col, old_scope) {
        (Some(col), Some(value)) => {
            values.push(Value::BigInt(Some(value)));
            where_clause.push_str(&format!(" AND {col} = ${}", values.len()));
        }
        (Some(col), None) => where_clause.push_str(&format!(" AND {col} IS NULL")),
        (None, _) => {}
    }
    execute(
        db,
        &format!(
            "UPDATE {table} SET {} WHERE {where_clause}",
            sets.join(", "),
            table = table,
        ),
        values,
    )
    .await?;

    // 旧父下后续兄弟前移（节点已移走，不会误伤它的子树）
    compact_tree_group_in_tx(db, spec, old_scope, &old_parent, args.operator_id).await?;

    assert_tree_group_unique(db, spec, target_scope, &target_parent).await?;
    assert_tree_group_unique(db, spec, old_scope, &old_parent).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_parent_strips_last_segment() {
        assert_eq!(tree_parent("001"), Some(""));
        assert_eq!(tree_parent("001002"), Some("001"));
        assert_eq!(tree_parent("001002003"), Some("001002"));
    }

    #[test]
    fn tree_parent_rejects_illegal_codes() {
        assert_eq!(tree_parent(""), None);
        assert_eq!(tree_parent("0010"), None);
        assert_eq!(tree_parent("00100"), None);
        assert_eq!(tree_parent("001a02"), None);
    }

    #[test]
    fn base36_pair_uses_two_chars() {
        assert_eq!(base36_pair(0), "~00");
        assert_eq!(base36_pair(35), "~0z");
        assert_eq!(base36_pair(36), "~10");
        assert_eq!(base36_pair(999), "~rr");
    }

    #[test]
    fn group_key_matches_lock_namespace() {
        assert_eq!(group_key("t", None, Some(7)), "t:*");
        assert_eq!(group_key("t", None, None), "t:*");
        assert_eq!(group_key("t", Some("g"), Some(7)), "t:g=7");
        assert_eq!(group_key("t", Some("g"), None), "t:g=null");
    }

    #[test]
    fn flat_sort_defaults() {
        let spec = FlatSort::new("t");
        assert_eq!(spec.id_col, "id");
        assert_eq!(spec.sort_col, "sort_order");
        assert_eq!(spec.group_col, None);
        assert_eq!(spec.updater_col, "updater_id");
        assert_eq!(spec.update_ms_col, "update_ms");
        assert_eq!(spec.max, SORT_ORDER_MAX);

        let grouped = FlatSort::new("t").with_group("group_id");
        assert_eq!(grouped.group_col, Some("group_id"));
    }

    #[test]
    fn tree_sort_defaults() {
        let spec = TreeSort::new("t");
        assert_eq!(spec.id_col, "id");
        assert_eq!(spec.code_col, "tree_code");
        assert_eq!(spec.scope_col, None);
        assert_eq!(spec.updater_col, "updater_id");
        assert_eq!(spec.update_ms_col, "update_ms");
        assert_eq!(spec.step, DEFAULT_TREE_STEP);
        assert_eq!(spec.max_length, DEFAULT_TREE_MAX_LENGTH);
        assert_eq!(spec.max_siblings, DEFAULT_TREE_MAX_SIBLINGS);

        let scoped = TreeSort::new("t").with_scope("app_id");
        assert_eq!(scoped.scope_col, Some("app_id"));
    }

    #[test]
    fn parent_value_semantics() {
        assert_eq!(
            Option::<Option<String>>::None.sort_parent(),
            None,
            "缺省表示不改父节点"
        );
        assert_eq!(
            Some(Option::<String>::None).sort_parent(),
            Some(String::new()),
            "显式 null 表示移动到根"
        );
        assert_eq!(
            Some(Some("001".to_string())).sort_parent(),
            Some("001".to_string())
        );
    }
}
