//! # 检索参数工具
//!
//! 本模块把「扁平请求参数」翻译为 sea-orm 的 `Condition` 与排序条件，
//! 供 DAO 层的 `search_*` 系列方法使用。所有与检索参数解析相关的特殊处理都收敛在本模块，
//! DAO / Svc / Ctrl 各层只负责调用，不重复实现。
//!
//! 本模块不依赖数据库连接，因此在仅启用 `api-client`（feign 客户端）时同样会编译：
//! [`SearchParams`] 实现了 [`serde::Serialize`]，会以扁平查询参数的形式序列化，
//! 可直接作为 feign 客户端 `search_*` 方法的查询参数。
//!
//! ## 字段参数规则
//!
//! 字段级参数沿用 Bean Searcher 的写法，以字段 `age` 为例：
//!
//! | 参数 | 说明 |
//! |---|---|
//! | `age=20` | 等价于 `age-0=20`，即 `age` 的第 0 个参数值 |
//! | `age-{n}=20` | `age` 的第 n 个参数值，如 `age-0`、`age-1` |
//! | `age-op=gt` | `age` 的[字段运算符](FieldOp)，支持缩写与全称 |
//! | `age-ic=true` | `age` 检索时是否忽略大小写 |
//!
//! ## 全局参数
//!
//! 全局参数只支持 robotech-rs 原生命名，统一以 `_` 前缀标识：
//!
//! | 参数 | 说明 | 缺省 |
//! |---|---|---|
//! | `_page` | 页码（从 1 开始） | 1 |
//! | `_size` | 页大小 | 10 |
//! | `_orderBy` | 排序表达式，如 `age:desc,name` | 无 |
//! | `_keyword` | 关键字，对 `like_columns` 做多列 OR 模糊匹配 | 无 |
//!
//! 其余以 `_` 开头的参数（如 `_current_user_id`）一律视为保留参数，不参与条件构建。
//!
//! Bean Searcher 的全局参数（`page` / `size` / `sort` / `order` / `keyword` /
//! `onlySelect` / `selectExclude`）**不支持**，以免与 robotech-rs 原生的
//! `_page`（1 基）/ `_size`（默认 10）语义冲突。
//!
//! ## 示例
//!
//! ```
//! use std::collections::HashMap;
//! use robotech::dao::{FieldOp, SearchParams};
//!
//! let query: HashMap<String, String> = [
//!     ("age".to_string(), "20".to_string()),
//!     ("age-op".to_string(), "gt".to_string()),
//!     ("_orderBy".to_string(), "age:desc".to_string()),
//!     ("_page".to_string(), "1".to_string()),
//! ]
//! .into_iter()
//! .collect();
//!
//! let params = SearchParams::from_query(&query);
//! assert_eq!(params.page().value(), 1);
//! assert_eq!(params.conds()[0].op, FieldOp::Gt);
//! ```

use crate::api::U64;
use sea_orm::prelude::Decimal;
use sea_orm::sea_query::{Expr, Func, Value};
use sea_orm::{ColumnTrait, ColumnType, Condition, ExprTrait, QueryOrder};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// # 字段参数分隔符
///
/// 字段参数形如 `age-0` / `age-op` / `age-ic`，此处定义分隔符 `-`。
pub const FIELD_SEPARATOR: char = '-';

/// # 字段运算符参数后缀
///
/// 字段参数形如 `age-op=gt`。
pub const OP_SUFFIX: &str = "op";

/// # 忽略大小写参数后缀
///
/// 字段参数形如 `age-ic=true`。
pub const IC_SUFFIX: &str = "ic";

/// # 默认页码（1 基）
pub const DEFAULT_PAGE_NUM: u64 = 1;

/// # 默认页大小
pub const DEFAULT_PAGE_SIZE: u64 = 10;

/// # 页码参数名（从 1 开始）
pub const PARAM_PAGE: &str = "_page";

/// # 页大小参数名
pub const PARAM_SIZE: &str = "_size";

/// # 排序表达式参数名
///
/// 形如 `age:desc,name`：以 `,` 分隔多个排序字段，字段后可用 `:asc` / `:desc`
/// 指定方向，未指定时默认为升序。
pub const PARAM_ORDER_BY: &str = "_orderBy";

/// # 排序表达式参数名（下划线写法）
///
/// 等价于 [`PARAM_ORDER_BY`]，两种写法都会被识别。
pub const PARAM_ORDER_BY_SNAKE: &str = "_order_by";

/// # 关键字参数名
///
/// 关键字会对 `like_columns` 指定的列做多列 OR 模糊匹配。
pub const PARAM_KEYWORD: &str = "_keyword";

/// # 字段运算符
///
/// 与 Bean Searcher 内置的字段运算符一一对应，用于描述某个字段的检索方式。
/// 每种运算符同时支持缩写与全称，解析时大小写不敏感。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldOp {
    /// 等于（缺省运算符）
    Eq,
    /// 不等于
    Ne,
    /// 大于
    Gt,
    /// 大于等于
    Ge,
    /// 小于
    Lt,
    /// 小于等于
    Le,
    /// 在...之间（范围查询）
    Bt,
    /// 不在...之间（范围查询）
    Nb,
    /// 包含（模糊查询）
    Ct,
    /// 以...开头（模糊查询）
    Sw,
    /// 以...结尾（模糊查询）
    Ew,
    /// 模糊或匹配（可带多个参数值）
    Ol,
    /// 反模糊匹配
    Nk,
    /// 多值查询（`in`）
    Il,
    /// 多值排除（`not in`）
    Ni,
    /// 为空（`is null`）
    Nl,
    /// 不为空（`is not null`）
    Nn,
    /// 为空（`is null or = ''`，仅适用于字符串字段）
    Ey,
    /// 不为空（`is not null and != ''`，仅适用于字符串字段）
    Ny,
    /// 恒真
    At,
    /// 恒假
    Af,
}

impl FieldOp {
    /// # 全部字段运算符
    pub const ALL: &'static [FieldOp] = &[
        Self::Eq,
        Self::Ne,
        Self::Gt,
        Self::Ge,
        Self::Lt,
        Self::Le,
        Self::Bt,
        Self::Nb,
        Self::Ct,
        Self::Sw,
        Self::Ew,
        Self::Ol,
        Self::Nk,
        Self::Il,
        Self::Ni,
        Self::Nl,
        Self::Nn,
        Self::Ey,
        Self::Ny,
        Self::At,
        Self::Af,
    ];

    /// # 运算符缩写
    ///
    /// ## 返回值
    /// 返回该运算符的缩写名称，例如 `FieldOp::Gt` 返回 `"gt"`
    pub fn name(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Gt => "gt",
            Self::Ge => "ge",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Bt => "bt",
            Self::Nb => "nb",
            Self::Ct => "ct",
            Self::Sw => "sw",
            Self::Ew => "ew",
            Self::Ol => "ol",
            Self::Nk => "nk",
            Self::Il => "il",
            Self::Ni => "ni",
            Self::Nl => "nl",
            Self::Nn => "nn",
            Self::Ey => "ey",
            Self::Ny => "ny",
            Self::At => "at",
            Self::Af => "af",
        }
    }

    /// # 解析字段运算符
    ///
    /// 支持缩写（如 `gt`）与全称（如 `GreaterThan`），大小写不敏感。
    ///
    /// ## 参数
    /// * `op` - 运算符字符串
    ///
    /// ## 返回值
    /// 解析成功返回对应的 `FieldOp`，无法识别时返回 `None`
    pub fn parse(op: &str) -> Option<Self> {
        let op = op.trim().to_ascii_lowercase();
        let parsed = match op.as_str() {
            "eq" | "equal" => Self::Eq,
            "ne" | "notequal" => Self::Ne,
            "gt" | "greaterthan" => Self::Gt,
            "ge" | "greaterequal" => Self::Ge,
            "lt" | "lessthan" => Self::Lt,
            "le" | "lessequal" => Self::Le,
            "bt" | "between" => Self::Bt,
            "nb" | "notbetween" => Self::Nb,
            "ct" | "contain" => Self::Ct,
            "sw" | "startwith" => Self::Sw,
            "ew" | "endwith" => Self::Ew,
            "ol" | "orlike" => Self::Ol,
            "nk" | "notlike" => Self::Nk,
            "il" | "inlist" | "in" => Self::Il,
            "ni" | "notin" => Self::Ni,
            "nl" | "isnull" => Self::Nl,
            "nn" | "notnull" => Self::Nn,
            "ey" | "empty" => Self::Ey,
            "ny" | "notempty" => Self::Ny,
            "at" | "alwaystrue" => Self::At,
            "af" | "alwaysfalse" => Self::Af,
            _ => return None,
        };
        Some(parsed)
    }

    /// # 是否为无需参数值的运算符
    ///
    /// `nl` / `nn` / `ey` / `ny` / `at` / `af` 不需要参数值即可构建条件。
    ///
    /// ## 返回值
    /// 无需参数值时返回 `true`
    pub fn is_valueless(self) -> bool {
        matches!(
            self,
            Self::Nl | Self::Nn | Self::Ey | Self::Ny | Self::At | Self::Af
        )
    }

    /// # 参数值为空时是否忽略该条件
    ///
    /// 与 Bean Searcher 的「是否忽略空值」列一致：除 `nl` / `nn` / `ey` / `ny` / `at` / `af`
    /// 之外的运算符，在参数值为 `null` 或空串时都会被忽略。
    ///
    /// ## 返回值
    /// 需要忽略空值时返回 `true`
    pub fn ignores_blank(self) -> bool {
        !self.is_valueless()
    }
}

/// # 单个字段的检索条件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldCond {
    /// 字段名（实体字段名，非数据库列名）
    pub field: String,
    /// 字段运算符
    pub op: FieldOp,
    /// 参数值列表（按 `field-{n}` 的下标升序排列）
    pub values: Vec<String>,
    /// 检索时是否忽略大小写
    pub ignore_case: bool,
}

impl FieldCond {
    /// # 创建字段条件
    ///
    /// ## 参数
    /// * `field` - 字段名
    /// * `op` - 字段运算符
    /// * `values` - 参数值列表
    /// * `ignore_case` - 是否忽略大小写
    ///
    /// ## 返回值
    /// 返回构建好的 `FieldCond`
    pub fn new(
        field: impl Into<String>,
        op: FieldOp,
        values: Vec<String>,
        ignore_case: bool,
    ) -> Self {
        Self {
            field: field.into(),
            op,
            values,
            ignore_case,
        }
    }

    /// # 将字段条件翻译为 sea-orm 条件
    ///
    /// 参数值会按目标列的类型自动转换（整型 / 浮点 / 布尔 / 数值），
    /// 无法识别的列类型按字符串处理。
    ///
    /// ## 参数
    /// * `col` - 该字段对应的数据库列
    ///
    /// ## 返回值
    /// 返回对应的 `Condition`；当参数值为空且该运算符忽略空值时返回 `None`
    pub fn to_condition<C>(&self, col: C) -> Option<Condition>
    where
        C: ColumnTrait,
    {
        let def = col.def();
        let col_type = def.get_column_type();

        // 无需参数值的运算符
        if self.op.is_valueless() {
            let expr = match self.op {
                FieldOp::Nl => col.into_expr().is_null(),
                FieldOp::Nn => col.into_expr().is_not_null(),
                FieldOp::Ey => col.into_expr().is_null().or(col.eq("")),
                FieldOp::Ny => col.into_expr().is_not_null().and(col.ne("")),
                FieldOp::At => return Some(Condition::all()),
                FieldOp::Af => return Some(Condition::all().add(Expr::val(0).eq(1))),
                _ => return None,
            };
            return Some(Condition::all().add(expr));
        }

        // 过滤空值，空值一律忽略
        let values: Vec<&str> = self
            .values
            .iter()
            .map(String::as_str)
            .filter(|v| !v.is_empty())
            .collect();
        if values.is_empty() {
            return None;
        }

        // 忽略大小写时统一走 lower(col) 比较
        let target: Expr = if self.ignore_case {
            Expr::from(Func::lower(col.into_expr()))
        } else {
            col.into_expr()
        };
        let first = values[0];
        let value = |raw: &str| to_value(col_type, raw);
        let lower = |raw: &str| Expr::val(raw.to_lowercase());

        let expr = match self.op {
            FieldOp::Eq => {
                if self.ignore_case {
                    target.eq(lower(first))
                } else {
                    col.eq(value(first))
                }
            }
            FieldOp::Ne => {
                if self.ignore_case {
                    target.ne(lower(first))
                } else {
                    col.ne(value(first))
                }
            }
            FieldOp::Gt => target.gt(Expr::val(value(first))),
            FieldOp::Ge => target.gte(Expr::val(value(first))),
            FieldOp::Lt => target.lt(Expr::val(value(first))),
            FieldOp::Le => target.lte(Expr::val(value(first))),
            // 范围查询：两个值取区间，单值退化为单边比较
            FieldOp::Bt => {
                if values.len() >= 2 {
                    col.between(value(values[0]), value(values[1]))
                } else {
                    target.gte(Expr::val(value(first)))
                }
            }
            FieldOp::Nb => {
                if values.len() >= 2 {
                    col.not_between(value(values[0]), value(values[1]))
                } else {
                    target.lt(Expr::val(value(first)))
                }
            }
            FieldOp::Ct => target.like(like_pattern(first, LikeKind::Contain, self.ignore_case)),
            FieldOp::Sw => target.like(like_pattern(first, LikeKind::StartWith, self.ignore_case)),
            FieldOp::Ew => target.like(like_pattern(first, LikeKind::EndWith, self.ignore_case)),
            FieldOp::Nk => target.not_like(like_pattern(first, LikeKind::Contain, self.ignore_case)),
            // 模糊或匹配：多值 OR
            FieldOp::Ol => {
                let mut any = Condition::any();
                for raw in &values {
                    any = any.add(
                        target
                            .clone()
                            .like(like_pattern(raw, LikeKind::Contain, self.ignore_case)),
                    );
                }
                return Some(Condition::all().add(any));
            }
            FieldOp::Il => {
                let list: Vec<Value> = values.iter().map(|raw| value(raw)).collect();
                col.is_in(list)
            }
            FieldOp::Ni => {
                let list: Vec<Value> = values.iter().map(|raw| value(raw)).collect();
                col.is_not_in(list)
            }
            _ => return None,
        };
        Some(Condition::all().add(expr))
    }
}

/// # LIKE 匹配方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LikeKind {
    /// 包含：`%value%`
    Contain,
    /// 开头：`value%`
    StartWith,
    /// 结尾：`%value`
    EndWith,
}

/// # 构建 LIKE 匹配模式串
///
/// ## 参数
/// * `raw` - 原始参数值
/// * `kind` - 匹配方式
/// * `ignore_case` - 是否忽略大小写（忽略时统一转为小写）
///
/// ## 返回值
/// 返回带通配符的匹配模式串
fn like_pattern(raw: &str, kind: LikeKind, ignore_case: bool) -> String {
    let value = if ignore_case {
        raw.to_lowercase()
    } else {
        raw.to_owned()
    };
    match kind {
        LikeKind::Contain => format!("%{value}%"),
        LikeKind::StartWith => format!("{value}%"),
        LikeKind::EndWith => format!("%{value}"),
    }
}

/// # 按列类型把字符串参数还原为数据库值
///
/// Bean Searcher 的参数是纯字符串，直接以字符串绑定到非文本列会导致
/// PostgreSQL 等强类型数据库报类型不匹配错误，因此这里按列类型做一次转换。
/// 无法识别的类型（含日期时间）一律按字符串处理。
///
/// ## 参数
/// * `col_type` - 目标列的类型
/// * `raw` - 原始参数值
///
/// ## 返回值
/// 返回转换后的 `Value`
fn to_value(col_type: &ColumnType, raw: &str) -> Value {
    match col_type {
        ColumnType::TinyInteger => Value::TinyInt(raw.parse().ok()),
        ColumnType::SmallInteger => Value::SmallInt(raw.parse().ok()),
        ColumnType::Integer => Value::Int(raw.parse().ok()),
        ColumnType::BigInteger => Value::BigInt(raw.parse().ok()),
        ColumnType::TinyUnsigned => Value::TinyUnsigned(raw.parse().ok()),
        ColumnType::SmallUnsigned => Value::SmallUnsigned(raw.parse().ok()),
        ColumnType::Unsigned => Value::Unsigned(raw.parse().ok()),
        ColumnType::BigUnsigned => Value::BigUnsigned(raw.parse().ok()),
        ColumnType::Float => Value::Float(raw.parse().ok()),
        ColumnType::Double => Value::Double(raw.parse().ok()),
        ColumnType::Boolean => match parse_bool(raw) {
            Some(b) => Value::Bool(Some(b)),
            None => Value::String(Some(raw.to_owned())),
        },
        ColumnType::Decimal(_) | ColumnType::Money(_) => match raw.parse::<Decimal>() {
            Ok(d) => Value::Decimal(Some(d)),
            Err(_) => Value::String(Some(raw.to_owned())),
        },
        _ => Value::String(Some(raw.to_owned())),
    }
}

/// # 解析布尔字面量
///
/// ## 参数
/// * `raw` - 原始参数值
///
/// ## 返回值
/// 可识别时返回 `Some(bool)`，否则返回 `None`
fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" | "on" => Some(true),
        "false" | "0" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

/// # 字段参数槽位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldSlot {
    /// 参数值，携带下标
    Value(u32),
    /// 字段运算符
    Op,
    /// 忽略大小写
    IgnoreCase,
}

/// # 拆分字段参数名
///
/// 把 `age`、`age-0`、`age-op`、`age-ic` 拆分为「字段名 + 槽位」。
///
/// ## 参数
/// * `key` - 参数名
///
/// ## 返回值
/// 返回字段名与槽位；无法解析时返回 `None`
fn split_field_key(key: &str) -> Option<(&str, FieldSlot)> {
    let Some(idx) = key.rfind(FIELD_SEPARATOR) else {
        return Some((key, FieldSlot::Value(0)));
    };
    let base = &key[..idx];
    let tail = &key[idx + FIELD_SEPARATOR.len_utf8()..];
    if base.is_empty() || tail.is_empty() {
        return None;
    }
    if tail.eq_ignore_ascii_case(OP_SUFFIX) {
        return Some((base, FieldSlot::Op));
    }
    if tail.eq_ignore_ascii_case(IC_SUFFIX) {
        return Some((base, FieldSlot::IgnoreCase));
    }
    if let Ok(idx) = tail.parse::<u32>() {
        return Some((base, FieldSlot::Value(idx)));
    }
    // 含分隔符但不是字段参数（如 `oss-obj`），整体作为字段名
    Some((key, FieldSlot::Value(0)))
}

/// # 解析排序表达式
///
/// 支持 robotech 原生格式 `age:desc,name`（未指定方向时默认升序）。
///
/// ## 参数
/// * `expr` - 排序表达式
///
/// ## 返回值
/// 返回 `(字段名, 是否升序)` 列表
fn parse_order_by_expr(expr: &str) -> Vec<(String, bool)> {
    expr.split(',')
        .filter_map(|item| {
            let item = item.trim();
            if item.is_empty() {
                return None;
            }
            if let Some(col) = item.strip_suffix(":desc") {
                Some((col.trim().to_owned(), false))
            } else if let Some(col) = item.strip_suffix(":asc") {
                Some((col.trim().to_owned(), true))
            } else {
                Some((item.to_owned(), true))
            }
        })
        .collect()
}

/// # 构建检索条件
///
/// 把检索参数中的字段条件与关键字（多列 OR 模糊匹配）合并为一个 `Condition`。
/// DAO 层的 `search_*` 方法统一通过本函数构建条件。
///
/// 本函数依赖 [`crate::dao::build_like_condition`]，仅在启用 `db` feature 时可用；
/// `api-client`（feign 客户端）侧只需要参数类型本身，不需要本函数。
///
/// ## 泛型参数
/// * `C` - 实体列类型，需实现 [`ColumnTrait`]
///
/// ## 参数
/// * `params` - 检索参数
/// * `like_columns` - 参与关键字模糊匹配的列集合
///
/// ## 返回值
/// 返回合并后的 `Condition`
#[cfg(feature = "db")]
pub fn build_search_condition<C>(params: &SearchParams, like_columns: &[C]) -> Condition
where
    C: ColumnTrait,
{
    let mut condition = params.to_condition::<C>();
    if let Some(keyword) = params.keyword() {
        condition = condition.add(crate::dao::build_like_condition(keyword, like_columns));
    }
    condition
}

/// # Bean Searcher 风格的检索参数
///
/// 由扁平请求参数解析而来，承载字段条件、排序、分页与关键字。
#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    /// 字段条件
    conds: Vec<FieldCond>,
    /// 页码（1 基）
    page: Option<u64>,
    /// 页大小
    size: Option<u64>,
    /// 排序条件：`(字段名, 是否升序)`
    sorts: Vec<(String, bool)>,
    /// 多字段模糊匹配关键字
    keyword: Option<String>,
}

impl SearchParams {
    /// # 创建空的检索参数
    ///
    /// ## 返回值
    /// 返回不含任何条件的 `SearchParams`
    pub fn new() -> Self {
        Self::default()
    }

    /// # 从扁平参数对解析检索参数
    ///
    /// 全局参数只识别 robotech-rs 原生的 `_page` / `_size` / `_orderBy` / `_order_by` /
    /// `_keyword`；其余以 `_` 开头的参数视为保留参数被跳过，
    /// 剩余参数按「字段参数规则」解析为字段条件。
    ///
    /// ## 参数
    /// * `pairs` - 参数名与参数值组成的迭代器
    ///
    /// ## 返回值
    /// 返回解析后的 `SearchParams`
    pub fn from_pairs<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut page: Option<u64> = None;
        let mut size: Option<u64> = None;
        let mut keyword: Option<String> = None;
        let mut order_by_expr: Option<String> = None;

        let mut op_map: BTreeMap<String, FieldOp> = BTreeMap::new();
        let mut ic_map: BTreeMap<String, bool> = BTreeMap::new();
        let mut value_map: BTreeMap<String, BTreeMap<u32, String>> = BTreeMap::new();

        for (key, value) in pairs {
            if key.is_empty() {
                continue;
            }
            match key.as_str() {
                PARAM_PAGE => {
                    page = value.trim().parse::<u64>().ok();
                    continue;
                }
                PARAM_SIZE => {
                    size = value.trim().parse::<u64>().ok();
                    continue;
                }
                PARAM_ORDER_BY | PARAM_ORDER_BY_SNAKE => {
                    order_by_expr = Some(value);
                    continue;
                }
                PARAM_KEYWORD => {
                    if !value.is_empty() {
                        keyword = Some(value);
                    }
                    continue;
                }
                _ => {}
            }
            // 其余以 `_` 前缀标识的参数（如 `_current_user_id`）视为保留参数，不参与字段条件构建
            if key.starts_with('_') {
                continue;
            }
            let Some((field, slot)) = split_field_key(&key) else {
                continue;
            };
            match slot {
                FieldSlot::Op => {
                    if let Some(op) = FieldOp::parse(&value) {
                        op_map.insert(field.to_owned(), op);
                    }
                }
                FieldSlot::IgnoreCase => {
                    ic_map.insert(field.to_owned(), parse_bool(&value).unwrap_or(false));
                }
                FieldSlot::Value(idx) => {
                    value_map
                        .entry(field.to_owned())
                        .or_default()
                        .insert(idx, value);
                }
            }
        }

        // 组装字段条件（字段名有序，保证结果可复现）
        let mut fields: BTreeSet<String> = BTreeSet::new();
        fields.extend(op_map.keys().cloned());
        fields.extend(ic_map.keys().cloned());
        fields.extend(value_map.keys().cloned());

        let mut conds: Vec<FieldCond> = Vec::new();
        for field in fields {
            let op = op_map.get(&field).copied().unwrap_or(FieldOp::Eq);
            let ignore_case = ic_map.get(&field).copied().unwrap_or(false);
            let values: Vec<String> = value_map
                .get(&field)
                .map(|map| map.values().cloned().collect())
                .unwrap_or_default();
            // 需要参数值却没有值的字段直接忽略
            if values.is_empty() && !op.is_valueless() {
                continue;
            }
            conds.push(FieldCond {
                field,
                op,
                values,
                ignore_case,
            });
        }

        // 排序表达式形如 `age:desc,name`
        let sorts = order_by_expr
            .map(|expr| parse_order_by_expr(&expr))
            .unwrap_or_default();

        Self {
            conds,
            page,
            size,
            sorts,
            keyword,
        }
    }

    /// # 从查询参数映射解析检索参数
    ///
    /// ## 参数
    /// * `query` - 查询参数映射（通常来自 HTTP 查询串）
    ///
    /// ## 返回值
    /// 返回解析后的 `SearchParams`
    pub fn from_query(query: &HashMap<String, String>) -> Self {
        Self::from_pairs(query.iter().map(|(k, v)| (k.clone(), v.clone())))
    }

    /// # 将检索参数展开为扁平参数对
    ///
    /// 输出格式与 [`SearchParams::from_pairs`] 的输入格式一致，可无损地重新解析；
    /// 同时也是 [`serde::Serialize`] 实现的数据来源。
    ///
    /// ## 返回值
    /// 返回 `(参数名, 参数值)` 列表：字段条件按字段名升序、参数值按下标升序输出，
    /// 全局参数（`_page` / `_size` / `_orderBy` / `_keyword`）仅在已设置时输出
    pub fn to_pairs(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = Vec::new();
        for cond in &self.conds {
            pairs.push((
                format!("{}{}{}", cond.field, FIELD_SEPARATOR, OP_SUFFIX),
                cond.op.name().to_owned(),
            ));
            if cond.ignore_case {
                pairs.push((
                    format!("{}{}{}", cond.field, FIELD_SEPARATOR, IC_SUFFIX),
                    "true".to_owned(),
                ));
            }
            for (idx, value) in cond.values.iter().enumerate() {
                pairs.push((
                    format!("{}{}{}", cond.field, FIELD_SEPARATOR, idx),
                    value.clone(),
                ));
            }
        }
        if let Some(page) = self.page {
            pairs.push((PARAM_PAGE.to_owned(), page.to_string()));
        }
        if let Some(size) = self.size {
            pairs.push((PARAM_SIZE.to_owned(), size.to_string()));
        }
        if !self.sorts.is_empty() {
            let expr = self
                .sorts
                .iter()
                .map(|(field, asc)| {
                    if *asc {
                        format!("{field}:asc")
                    } else {
                        format!("{field}:desc")
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            pairs.push((PARAM_ORDER_BY.to_owned(), expr));
        }
        if let Some(keyword) = &self.keyword {
            pairs.push((PARAM_KEYWORD.to_owned(), keyword.clone()));
        }
        pairs
    }

    /// # 将检索参数展开为查询参数映射
    ///
    /// 便于直接交给 HTTP 客户端作为 query string 使用。
    ///
    /// ## 返回值
    /// 返回「参数名 → 参数值」映射
    pub fn to_query_map(&self) -> HashMap<String, String> {
        self.to_pairs().into_iter().collect()
    }

    /// # 追加字段条件
    ///
    /// ## 参数
    /// * `cond` - 待追加的字段条件
    ///
    /// ## 返回值
    /// 返回追加后的 `SearchParams`（便于链式调用）
    pub fn add_cond(mut self, cond: FieldCond) -> Self {
        self.conds.push(cond);
        self
    }

    /// # 追加字段条件（快捷方式）
    ///
    /// ## 参数
    /// * `field` - 字段名
    /// * `op` - 字段运算符
    /// * `values` - 参数值列表
    ///
    /// ## 返回值
    /// 返回追加后的 `SearchParams`
    pub fn with_cond(mut self, field: impl Into<String>, op: FieldOp, values: Vec<String>) -> Self {
        self.conds.push(FieldCond::new(field, op, values, false));
        self
    }

    /// # 追加排序条件
    ///
    /// ## 参数
    /// * `field` - 排序字段
    /// * `asc` - 是否升序
    ///
    /// ## 返回值
    /// 返回追加后的 `SearchParams`
    pub fn with_sort(mut self, field: impl Into<String>, asc: bool) -> Self {
        self.sorts.push((field.into(), asc));
        self
    }

    /// # 设置分页
    ///
    /// ## 参数
    /// * `page` - 页码（从 1 开始）
    /// * `size` - 页大小
    ///
    /// ## 返回值
    /// 返回设置后的 `SearchParams`
    pub fn with_page(mut self, page: u64, size: u64) -> Self {
        self.page = Some(page);
        self.size = Some(size);
        self
    }

    /// # 设置关键字
    ///
    /// ## 参数
    /// * `keyword` - 多字段模糊匹配关键字
    ///
    /// ## 返回值
    /// 返回设置后的 `SearchParams`
    pub fn with_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.keyword = Some(keyword.into());
        self
    }

    /// # 获取字段条件列表
    pub fn conds(&self) -> &[FieldCond] {
        &self.conds
    }

    /// # 获取关键字
    ///
    /// ## 返回值
    /// 未设置关键字时返回 `None`
    pub fn keyword(&self) -> Option<&str> {
        self.keyword.as_deref()
    }

    /// # 获取页码
    ///
    /// 页码从 1 开始（与 robotech-rs 原生 `_page` 一致）。
    ///
    /// ## 返回值
    /// 返回页码，缺省为 [`DEFAULT_PAGE_NUM`]，最小为 1
    pub fn page(&self) -> U64 {
        U64(match self.page {
            Some(page) if page > 0 => page,
            _ => DEFAULT_PAGE_NUM,
        })
    }

    /// # 获取页大小
    ///
    /// ## 返回值
    /// 返回页大小，缺省为 [`DEFAULT_PAGE_SIZE`]，最小为 1
    pub fn size(&self) -> U64 {
        U64(match self.size {
            Some(size) if size > 0 => size,
            _ => DEFAULT_PAGE_SIZE,
        })
    }

    /// # 是否存在检索条件
    ///
    /// ## 返回值
    /// 存在字段条件或关键字时返回 `true`
    pub fn has_condition(&self) -> bool {
        !self.conds.is_empty() || self.keyword.is_some()
    }

    /// # 将检索条件翻译为 sea-orm 条件
    ///
    /// 无法映射为数据库列的字段会被静默忽略。
    ///
    /// ## 泛型参数
    /// * `C` - 实体列类型，需实现 [`ColumnTrait`]
    ///
    /// ## 返回值
    /// 返回由各字段条件以 AND 连接组成的 `Condition`
    pub fn to_condition<C>(&self) -> Condition
    where
        C: ColumnTrait,
    {
        let mut condition = Condition::all();
        for cond in &self.conds {
            let Ok(col) = cond.field.parse::<C>() else {
                continue;
            };
            if let Some(sub) = cond.to_condition(col) {
                condition = condition.add(sub);
            }
        }
        condition
    }

    /// # 为查询追加排序条件
    ///
    /// 无法映射为数据库列的排序字段会被静默忽略。
    ///
    /// ## 泛型参数
    /// * `C` - 实体列类型，需实现 [`ColumnTrait`]
    /// * `Q` - 查询对象类型，需实现 [`QueryOrder`]
    ///
    /// ## 参数
    /// * `query` - 待追加排序的查询对象
    ///
    /// ## 返回值
    /// 返回追加排序条件后的查询对象
    pub fn apply_order_by<C, Q>(&self, mut query: Q) -> Q
    where
        C: ColumnTrait,
        Q: QueryOrder,
    {
        for (field, asc) in &self.sorts {
            let Ok(col) = field.parse::<C>() else {
                continue;
            };
            query = if *asc {
                query.order_by_asc(col)
            } else {
                query.order_by_desc(col)
            };
        }
        query
    }
}

/// # 以扁平查询参数的形式序列化
///
/// 序列化为 `{ "age-op": "gt", "age-0": "20", "_page": "1", ... }` 形式的 map，
/// 便于直接作为 HTTP 客户端的 query 参数使用（例如 feign 客户端的 `search_*` 方法）。
/// 序列化结果与 [`SearchParams::from_pairs`] 的输入格式一致，可往返还原。
impl serde::Serialize for SearchParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let pairs = self.to_pairs();
        let mut map = serializer.serialize_map(Some(pairs.len()))?;
        for (key, value) in &pairs {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_of(pairs: &[(&str, &str)]) -> SearchParams {
        SearchParams::from_pairs(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
        )
    }

    #[test]
    fn test_field_op_parse() {
        assert_eq!(FieldOp::parse("gt"), Some(FieldOp::Gt));
        assert_eq!(FieldOp::parse("GreaterThan"), Some(FieldOp::Gt));
        assert_eq!(FieldOp::parse("GREATER_EQUAL"), None);
        assert_eq!(FieldOp::parse("ge"), Some(FieldOp::Ge));
        assert_eq!(FieldOp::parse("between"), Some(FieldOp::Bt));
        assert_eq!(FieldOp::parse("il"), Some(FieldOp::Il));
        assert_eq!(FieldOp::parse("unknown"), None);
        assert_eq!(FieldOp::ALL.len(), 21);
    }

    #[test]
    fn test_field_op_flags() {
        assert!(FieldOp::Nl.is_valueless());
        assert!(!FieldOp::Nl.ignores_blank());
        assert!(FieldOp::Eq.ignores_blank());
        assert!(!FieldOp::Eq.is_valueless());
        assert_eq!(FieldOp::Ny.name(), "ny");
    }

    #[test]
    fn test_parse_page() {
        // 原生 _page 从 1 开始，不做任何偏移
        let params = params_of(&[("_page", "5")]);
        assert_eq!(params.page().value(), 5);
        // 0 或非法值回退到默认页码
        assert_eq!(params_of(&[("_page", "0")]).page().value(), DEFAULT_PAGE_NUM);
        assert_eq!(params_of(&[("_page", "x")]).page().value(), DEFAULT_PAGE_NUM);
        // 缺省值
        assert_eq!(SearchParams::new().page().value(), DEFAULT_PAGE_NUM);
        assert_eq!(SearchParams::new().size().value(), DEFAULT_PAGE_SIZE);
        // 0 或非法页大小回退到默认页大小
        assert_eq!(params_of(&[("_size", "0")]).size().value(), DEFAULT_PAGE_SIZE);
    }

    #[test]
    fn test_parse_field_cond() {
        let params = params_of(&[("age", "20"), ("age-op", "gt")]);
        assert_eq!(params.conds().len(), 1);
        assert_eq!(params.conds()[0].field, "age");
        assert_eq!(params.conds()[0].op, FieldOp::Gt);
        assert_eq!(params.conds()[0].values, vec!["20".to_string()]);
    }

    #[test]
    fn test_parse_multi_value_ordered_by_index() {
        let params = params_of(&[("age-1", "30"), ("age-0", "20"), ("age-op", "bt")]);
        assert_eq!(params.conds().len(), 1);
        assert_eq!(params.conds()[0].op, FieldOp::Bt);
        assert_eq!(
            params.conds()[0].values,
            vec!["20".to_string(), "30".to_string()]
        );
    }

    #[test]
    fn test_parse_ignore_case() {
        let params = params_of(&[("name", "Jack"), ("name-ic", "true")]);
        assert_eq!(params.conds().len(), 1);
        assert!(params.conds()[0].ignore_case);
    }

    #[test]
    fn test_parse_valueless_op() {
        // name-op=ey 不带参数值也应生成条件
        let params = params_of(&[("name-op", "ey")]);
        assert_eq!(params.conds().len(), 1);
        assert_eq!(params.conds()[0].op, FieldOp::Ey);
        assert!(params.conds()[0].values.is_empty());
    }

    #[test]
    fn test_parse_skip_op_without_value() {
        // 需要参数值但没有值的字段被忽略
        let params = params_of(&[("age-op", "gt")]);
        assert!(params.conds().is_empty());
    }

    #[test]
    fn test_parse_reserved_params_ignored() {
        let params = params_of(&[
            ("_current_user_id", "1"),
            ("_page", "2"),
            ("_size", "20"),
            ("age", "20"),
        ]);
        assert_eq!(params.conds().len(), 1);
        assert_eq!(params.conds()[0].field, "age");
        assert_eq!(params.page().value(), 2);
        assert_eq!(params.size().value(), 20);
    }

    #[test]
    fn test_bean_searcher_global_params_not_supported() {
        // Bean Searcher 的全局参数不再识别，只会退化成普通字段条件（最终因列名不匹配被忽略）
        let params = params_of(&[("page", "0"), ("size", "15"), ("sort", "age")]);
        assert_eq!(params.page().value(), DEFAULT_PAGE_NUM);
        assert_eq!(params.size().value(), DEFAULT_PAGE_SIZE);
        assert!(params.sorts.is_empty());
        assert_eq!(params.conds().len(), 3);
    }

    #[test]
    fn test_parse_order_by() {
        let params = params_of(&[("_orderBy", "age:desc,name")]);
        assert_eq!(
            params.sorts,
            vec![("age".to_string(), false), ("name".to_string(), true)]
        );
        // 下划线写法等价
        let params = params_of(&[("_order_by", "age:asc")]);
        assert_eq!(params.sorts, vec![("age".to_string(), true)]);
    }

    #[test]
    fn test_parse_keyword() {
        assert_eq!(params_of(&[("_keyword", "jack")]).keyword(), Some("jack"));
        assert_eq!(params_of(&[("_keyword", "")]).keyword(), None);
        assert!(!SearchParams::new().has_condition());
        assert!(params_of(&[("_keyword", "jack")]).has_condition());
    }

    #[test]
    fn test_to_value_by_column_type() {
        assert_eq!(
            to_value(&ColumnType::BigInteger, "20"),
            Value::BigInt(Some(20))
        );
        assert_eq!(to_value(&ColumnType::Integer, "20"), Value::Int(Some(20)));
        assert_eq!(
            to_value(&ColumnType::Boolean, "true"),
            Value::Bool(Some(true))
        );
        assert_eq!(
            to_value(&ColumnType::String(sea_orm::sea_query::StringLen::None), "jack"),
            Value::String(Some("jack".to_string()))
        );
    }

    #[test]
    fn test_like_pattern() {
        assert_eq!(like_pattern("Jack", LikeKind::Contain, false), "%Jack%");
        assert_eq!(like_pattern("Jack", LikeKind::StartWith, false), "Jack%");
        assert_eq!(like_pattern("Jack", LikeKind::EndWith, false), "%Jack");
        assert_eq!(like_pattern("Jack", LikeKind::Contain, true), "%jack%");
    }

    #[test]
    fn test_split_field_key() {
        assert_eq!(split_field_key("age"), Some(("age", FieldSlot::Value(0))));
        assert_eq!(split_field_key("age-2"), Some(("age", FieldSlot::Value(2))));
        assert_eq!(split_field_key("age-op"), Some(("age", FieldSlot::Op)));
        assert_eq!(split_field_key("age-ic"), Some(("age", FieldSlot::IgnoreCase)));
        // 含分隔符但不是字段参数
        assert_eq!(
            split_field_key("oss-obj"),
            Some(("oss-obj", FieldSlot::Value(0)))
        );
        assert_eq!(split_field_key("-op"), None);
        assert_eq!(split_field_key("age-"), None);
    }

    #[test]
    fn test_to_pairs_round_trip() {
        let params = SearchParams::from_pairs(
            [
                ("age".to_string(), "20".to_string()),
                ("age-op".to_string(), "gt".to_string()),
                ("name".to_string(), "Jack".to_string()),
                ("name-op".to_string(), "ct".to_string()),
                ("name-ic".to_string(), "true".to_string()),
                ("_page".to_string(), "2".to_string()),
                ("_size".to_string(), "20".to_string()),
                ("_orderBy".to_string(), "age:desc,name".to_string()),
                ("_keyword".to_string(), "jack".to_string()),
            ]
            .into_iter(),
        );

        let pairs = params.to_pairs();
        assert!(pairs.contains(&("age-op".to_string(), "gt".to_string())));
        assert!(pairs.contains(&("age-0".to_string(), "20".to_string())));
        assert!(pairs.contains(&("name-op".to_string(), "ct".to_string())));
        assert!(pairs.contains(&("name-ic".to_string(), "true".to_string())));
        assert!(pairs.contains(&("name-0".to_string(), "Jack".to_string())));
        assert!(pairs.contains(&("_page".to_string(), "2".to_string())));
        assert!(pairs.contains(&("_size".to_string(), "20".to_string())));
        assert!(pairs.contains(&("_orderBy".to_string(), "age:desc,name:asc".to_string())));
        assert!(pairs.contains(&("_keyword".to_string(), "jack".to_string())));

        // 往返：to_pairs 的结果可被 from_pairs 还原
        let restored = SearchParams::from_pairs(pairs);
        assert_eq!(restored.conds(), params.conds());
        assert_eq!(restored.page().value(), params.page().value());
        assert_eq!(restored.size().value(), params.size().value());
        assert_eq!(restored.sorts, params.sorts);
        assert_eq!(restored.keyword(), params.keyword());
    }

    #[test]
    fn test_to_pairs_skips_unset_globals() {
        // 未设置的全局参数不输出
        let pairs = SearchParams::new().to_pairs();
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_serialize_as_flat_query_map() {
        let params =
            SearchParams::from_pairs([("age".to_string(), "20".to_string())].into_iter());
        let value = serde_json::to_value(&params).unwrap();
        // 序列化为扁平 map，可直接作为 HTTP 客户端 query 参数
        assert_eq!(value["age-op"], serde_json::json!("eq"));
        assert_eq!(value["age-0"], serde_json::json!("20"));
        assert_eq!(
            params.to_query_map().get("age-0").map(String::as_str),
            Some("20")
        );
    }
}
