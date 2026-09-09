// robotech/src/dao/belongs_to_utils.rs
use sea_orm::{
    compound::{BelongsTo, HasMany, HasOne},
    EntityTrait,
};

// ================================================================
// 所有权版本（配合 o2o from_owned，零拷贝性能优先）
// ================================================================

/// # 多对一 BelongsTo 所有权转 VO（必选外键）
///
/// 将已加载的 `BelongsTo<E>` 关联转换为 VO（值对象）。
/// 由于外键为 NOT NULL 且有外键约束，数据必定存在，未加载时直接 panic。
pub fn belongs_to_owned<E, V>(b: BelongsTo<E>) -> V
where
    E: EntityTrait,
    V: From<E::ModelEx>,
{
    match b {
        BelongsTo::Loaded(model_ex) => V::from(*model_ex),
        BelongsTo::Unloaded => panic!("BelongsTo relation not loaded"),
    }
}

/// # 多对一 BelongsTo 所有权转 VO（可选外键）
///
/// 将已加载的 `BelongsTo<Option<E>>` 关联转换为可选的 VO，
/// 外键为 NULL 或未加载时返回 `None`。
pub fn belongs_to_opt_owned<E, V>(b: BelongsTo<Option<E>>) -> Option<V>
where
    E: EntityTrait,
    V: From<E::ModelEx>,
{
    match b {
        BelongsTo::Loaded(Some(model_ex)) => Some(V::from(*model_ex)),
        _ => None,
    }
}

/// # 一对一 HasOne 所有权转 VO（天然可选）
///
/// 将已加载的 `HasOne<E>` 关联转换为可选的 VO，未加载或无数据时返回 `None`。
pub fn has_one_opt_owned<E, V>(h: HasOne<E>) -> Option<V>
where
    E: EntityTrait,
    V: From<E::ModelEx>,
{
    match h {
        HasOne::Loaded(Some(model_ex)) => Some(V::from(*model_ex)),
        _ => None,
    }
}

/// # 一对多 HasMany 所有权转 VO 列表
///
/// 将已加载的 `HasMany<E>` 关联列表转换为 VO 列表，未加载时返回空列表。
pub fn has_many_owned<E, V>(h: HasMany<E>) -> Vec<V>
where
    E: EntityTrait,
    V: From<E::ModelEx>,
{
    match h {
        HasMany::Loaded(list) => list.into_iter().map(V::from).collect(),
        HasMany::Unloaded => vec![],
    }
}

// ================================================================
// 引用版本（配合 o2o from_ref，保留原对象）
// ================================================================

/// # 多对一 BelongsTo 引用转 VO（必选外键）
///
/// 将已加载的 `BelongsTo<E>` 关联按引用转换为可选的 VO，未加载时返回 `None`。
pub fn belongs_to_ref<E, V>(b: &BelongsTo<E>) -> Option<V>
where
    E: EntityTrait,
    for<'a> V: From<&'a E::ModelEx>,
{
    match b {
        BelongsTo::Loaded(model_ex) => Some(V::from(model_ex.as_ref())),
        BelongsTo::Unloaded => None,
    }
}

/// # 多对一 BelongsTo 引用转 VO（可选外键）
///
/// 将已加载的 `BelongsTo<Option<E>>` 关联按引用转换为 VO，
/// 未加载或外键值为 `None` 时直接 panic。
pub fn belongs_to_opt_ref<E, V>(b: &BelongsTo<Option<E>>) -> V
where
    E: EntityTrait,
    for<'a> V: From<&'a E::ModelEx>,
{
    match b {
        BelongsTo::Loaded(Some(model_ex)) => V::from(model_ex.as_ref()),
        _ => panic!("BelongsTo relation not loaded or is None"),
    }
}

/// # 一对一 HasOne 引用转 VO
///
/// 将已加载的 `HasOne<E>` 关联按引用转换为可选的 VO，未加载或无数据时返回 `None`。
pub fn has_one_opt_ref<E, V>(h: &HasOne<E>) -> Option<V>
where
    E: EntityTrait,
    for<'a> V: From<&'a E::ModelEx>,
{
    match h {
        HasOne::Loaded(Some(model_ex)) => Some(V::from(model_ex.as_ref())),
        _ => None,
    }
}

/// # 一对多 HasMany 引用转 VO 列表
///
/// 将已加载的 `HasMany<E>` 关联列表按引用转换为 VO 列表，未加载时返回空列表。
pub fn has_many_ref<E, V>(h: &HasMany<E>) -> Vec<V>
where
    E: EntityTrait,
    for<'a> V: From<&'a E::ModelEx>,
{
    match h {
        HasMany::Loaded(list) => list.iter().map(V::from).collect(),
        HasMany::Unloaded => vec![],
    }
}

// ================================================================
// 批量转换通用工具
// ================================================================

/// # 批量所有权转换
///
/// 将实体列表整体转换为 VO 列表，转换逻辑由 `From<E>` 实现提供。
///
/// ## 参数
/// * `list` - 待转换的实体列表
///
/// ## 返回值
/// 转换后的 VO 列表
pub fn convert_list_owned<E, V>(list: Vec<E>) -> Vec<V>
where
    V: From<E>,
{
    list.into_iter().map(V::from).collect()
}

/// # 批量引用转换
///
/// 将实体切片中的元素按引用转换为 VO 列表，转换逻辑由 `From<&E>` 实现提供。
///
/// ## 参数
/// * `list` - 待转换的实体切片
///
/// ## 返回值
/// 转换后的 VO 列表
pub fn convert_list_ref<E, V>(list: &[E]) -> Vec<V>
where
    for<'a> V: From<&'a E>,
{
    list.iter().map(V::from).collect()
}