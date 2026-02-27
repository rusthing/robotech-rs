mod cfg;
mod dao;
mod log;
mod svc;
mod web;

use crate::cfg::{watch_cfg_file_macro, WatchCfgFileArgs};
use crate::dao::{dao_macro, DaoArgs};
use crate::log::{log_call_macro, LogCallArgs};
use crate::svc::{db_unwrap_macro, svc_macro, DbUnwrapArgs, SvcArgs};
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
pub fn log_call(attr: TokenStream, item: TokenStream) -> TokenStream {
    // 解析属性参数
    let args = parse_macro_input!(attr as LogCallArgs);
    let input = parse_macro_input!(item as ItemFn);
    log_call_macro(args, input).into()
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
pub fn dao(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as DaoArgs);
    let input = parse_macro_input!(item as ItemStruct);
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
pub fn db_unwrap(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as DbUnwrapArgs);
    let input = parse_macro_input!(item as ItemFn);
    db_unwrap_macro(args, input).into()
}

#[proc_macro_attribute]
pub fn svc(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as SvcArgs);
    let input = parse_macro_input!(item as ItemStruct);
    svc_macro(args, input).into()
}

#[proc_macro_attribute]
pub fn ctrl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as CtrlArgs);
    let input = parse_macro_input!(item as ItemStruct);
    ctrl_macro(args, input).into()
}
