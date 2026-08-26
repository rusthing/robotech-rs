// robotech/src/dao/belongs_to_utils.rs
use sea_orm::{
    compound::{BelongsTo, HasMany, HasOne},
    EntityTrait,
};

// ================================================================
// 所有权版本（配合 o2o from_owned，零拷贝性能优先）
// ================================================================

/// 多对一 BelongsTo（必选外键，NOT NULL）所有权转 VO
/// 有外键约束，数据必定存在，未加载则 panic
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

/// 多对一 BelongsTo（可选外键，nullable）所有权转 VO
/// 对应实体类型：BelongsTo<Option<Entity>>
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

/// 一对一 HasOne 所有权转 VO（天然可选）
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

/// 一对多 HasMany 所有权转 VO 列表
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

/// 多对一 BelongsTo（必选外键）引用转 VO
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

/// 多对一 BelongsTo（可选外键）引用转 VO
/// 有外键约束，数据必定存在，未加载则 panic
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

/// 一对一 HasOne 引用转 VO
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

/// 一对多 HasMany 引用转 VO 列表
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

pub fn convert_list_owned<E, V>(list: Vec<E>) -> Vec<V>
where
    V: From<E>,
{
    list.into_iter().map(V::from).collect()
}

pub fn convert_list_ref<E, V>(list: &[E]) -> Vec<V>
where
    for<'a> V: From<&'a E>,
{
    list.iter().map(V::from).collect()
}