//! # 添加时传入 id 是否生效
//!
//! 不需要数据库：只验框架的 `AddDto -> ActiveModel` 转换，确认 AddDto 里显式给的 id
//! 会被原样带进 `ActiveModel`（`ActiveValue::Set`），而不是 `NotSet`（交给 DAO 走
//! `idworker` 重新生成）。
//!
//! 这是用户关心的问题的核心：微服务 add 时传了 id，是按指定 id 落库，还是被忽略、
//! 自动再生成一个。

use robotech::api::U64;
use robotech_tests::dto::tx_item_dto::TxItemAddDto;
use robotech_tests::mo::tx_item::ActiveModel;
use sea_orm::ActiveValue;

#[test]
fn add_dto_explicit_id_is_carried_into_active_model() {
    let mut dto = TxItemAddDto::default();
    dto.id = Some(U64(123456789));
    dto.code = Some("explicit-id".to_string());
    dto._current_user_id = U64::from(1u64);

    // svc::add 内部正是 `let active_model: ActiveModel = add_dto.clone().into();`
    let am: ActiveModel = dto.into();

    match am.id {
        ActiveValue::Set(v) => {
            assert_eq!(v, 123456789, "AddDto 的 id 必须原样进入 ActiveModel")
        }
        ActiveValue::NotSet => panic!("AddDto 的 id 被忽略，会被 DAO 重新生成"),
        ActiveValue::Unchanged(_) => panic!("id 不应是 Unchanged 形态"),
    }
}

#[test]
fn add_dto_absent_id_is_not_set() {
    let mut dto = TxItemAddDto::default();
    dto.code = Some("auto-id".to_string());
    dto._current_user_id = U64::from(1u64);

    let am: ActiveModel = dto.into();
    assert!(
        matches!(am.id, ActiveValue::NotSet),
        "没传 id 时应当是 NotSet，由 DAO 用 idworker 补齐"
    );
}
