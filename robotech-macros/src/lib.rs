mod cfg;
mod dao;
mod db;
mod dto;
mod log;
mod svc;
mod vo;
mod web;

use crate::cfg::{watch_cfg_file_macro, WatchCfgFileArgs};
use crate::dao::{
    dao_macro, define_foreign_keys_macro, define_like_columns_macro, define_unique_fields_macro, DaoArgs,
    DefineForeignKeysArgs, DefineLikeColumnsArgs, DefineUniqueFieldsArgs,
};
use crate::db::MigrateArgs;
use crate::dto::{add_dto_macro, crud_dto_macro, modify_dto_macro, save_dto_macro};
use crate::log::{log_call_macro, LogCallArgs};
use crate::svc::{db_unwrap_macro, svc_macro, DbUnwrapArgs, SvcArgs};
use crate::vo::vo_macro;
use crate::web::{ctrl_macro, CtrlArgs};
use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemFn, ItemStruct};

#[proc_macro]
pub fn watch_cfg_file(args: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as WatchCfgFileArgs);
    watch_cfg_file_macro(args).into()
}

/// 属性宏：在进入方法时使用 log 库记录方法名、参数及参数值
///
/// # 使用示例
/// ```
/// // 使用默认 debug 级别
/// #[log_call]
/// fn add(a: i32, b: i32) -> i32 {
///     a + b
/// }
///
/// // 指定日志级别
/// #[log_call(level = info)]
/// fn process(data: &str) {
///     // ...
/// }
///
/// #[log_call(level = warn)]
/// fn risky_operation() {
///     // ...
/// }
/// ```
///
/// 支持的日志级别: trace, debug (默认), info, warn, error
#[proc_macro_attribute]
pub fn log_call(args: TokenStream, input: TokenStream) -> TokenStream {
    // 解析属性参数
    let args = parse_macro_input!(args as LogCallArgs);
    let input = parse_macro_input!(input as ItemFn);
    log_call_macro(args, input).into()
}

#[proc_macro]
pub fn db_migrate(args: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as MigrateArgs);
    db::db_migrate_macro(args).into()
}

#[proc_macro_attribute]
pub fn add_dto(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    add_dto_macro(input).into()
}

#[proc_macro_attribute]
pub fn modify_dto(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    modify_dto_macro(input).into()
}

#[proc_macro_attribute]
pub fn save_dto(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    save_dto_macro(input).into()
}

/// 属性宏：为XxxDto结构体自动生成XxxAddDto、XxxModifyDto、XxxSaveDto
///
/// # 使用示例
/// ```
/// #[crud_dto]
/// pub struct OssBucketDto {
///     /// 名称
///     pub name: String,
///     /// 备注
///     pub remark: Option<String>,
/// }
/// ```
///
/// 上述代码会被展开为三个结构体：
/// - OssBucketAddDto（带验证）
/// - OssBucketModifyDto（不带验证）
/// - OssBucketSaveDto（不带验证）
#[proc_macro_attribute]
pub fn crud_dto(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    crud_dto_macro(input).into()
}

#[proc_macro]
pub fn define_unique_fields(args: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as DefineUniqueFieldsArgs);
    define_unique_fields_macro(args).into()
}

#[proc_macro]
pub fn define_foreign_keys(args: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as DefineForeignKeysArgs);
    define_foreign_keys_macro(args).into()
}

#[proc_macro]
pub fn define_like_columns(args: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as DefineLikeColumnsArgs);
    define_like_columns_macro(args).into()
}

/// 属性宏：为DAO结构体生成标准的CRUD方法
///
/// # 使用示例
/// ```
/// // 生成所有方法
/// #[dao(all)]
/// pub struct MyDao;
///
/// // 选择性生成方法
/// #[dao(insert, update, get_by_id)]
/// pub struct MyDao;
///
/// // 只生成查询方法
/// #[dao(get_by_id)]
/// pub struct MyDao;
/// ```
///
/// 支持的方法选项:
/// - insert: 生成插入方法
/// - update: 生成更新方法
/// - delete: 生成删除方法
/// - get_by_id: 生成根据ID查询方法
/// - all: 生成所有方法
#[proc_macro_attribute]
pub fn dao(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as DaoArgs);
    let input = parse_macro_input!(input as ItemStruct);
    dao_macro(args, input).into()
}

/// 属性宏：为Service查询方法生成标准结构
///
/// 此宏会自动处理数据库连接逻辑，用户只需编写返回语句
///
/// # 使用示例
/// ```
/// #[db_unwrap]
/// pub async fn get_by_name<C>(name: &str, db: Option<&C>) -> Result<Ro<OssBucketVo>, SvcError>
/// where
///     C: ConnectionTrait,
/// {
///     let one = OssBucketDao::get_by_name(name, db).await?;
///     Ok(
///         Ro::success("查询成功".to_string())
///             .extra(one.map(|value| OssBucketVo::from(value))),
///     )
/// }
/// ```
/// 注意：用户代码中应该包含完整的返回逻辑
#[proc_macro_attribute]
pub fn db_unwrap(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as DbUnwrapArgs);
    let input = parse_macro_input!(input as ItemFn);
    db_unwrap_macro(args, input).into()
}

#[proc_macro_attribute]
pub fn svc(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as SvcArgs);
    let input = parse_macro_input!(input as ItemStruct);
    svc_macro(args, input).into()
}

#[proc_macro_attribute]
pub fn ctrl(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as CtrlArgs);
    let input = parse_macro_input!(input as ItemStruct);
    ctrl_macro(args, input).into()
}

/// 属性宏：为 VO 结构体自动生成标准属性
///
/// 此宏会自动为 VO 结构体添加以下属性和派生宏：
/// - `#[skip_serializing_none]` - 跳过空字段序列化
/// - `#[derive(o2o, ToSchema, Debug, Serialize, Clone)]` - 必要的派生宏
/// - `#[from_owned(Model)]` - o2o 转换配置
/// - `#[serde(rename_all = "camelCase")]` - 驼峰命名
/// - `#[serde_as]` - serde_with 支持
///
/// 同时会自动为无符号整型字段添加：
/// - `#[from(~ as u64)]` 或 `#[from(~.to_string())]` - 根据字段名自动判断
/// - `#[serde_as(as = "String")]` - 避免 JS 精度丢失
///
/// # 使用示例
/// ```
/// #[vo]
/// pub struct StudentVo {
///     /// ID
///     pub id: u64,
///     /// 名称
///     pub name: String,
///     /// 备注
///     pub remark: Option<String>,
/// }
/// ```
///
/// 上述代码会被展开为：
/// ```
/// #[skip_serializing_none]
/// #[derive(o2o, ToSchema, Debug, Serialize, Clone)]
/// #[from_owned(Model)]
/// #[serde(rename_all = "camelCase")]
/// #[serde_as]
/// pub struct StudentVo {
///     /// ID
///     #[from(~ as u64)]
///     #[serde_as(as = "String")]
///     pub id: u64,
///     /// 名称
///     pub name: String,
///     /// 备注
///     pub remark: Option<String>,
/// }
/// ```
#[proc_macro_attribute]
pub fn vo(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    vo_macro(input).into()
}
