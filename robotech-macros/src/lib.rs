mod dao;
mod db;
mod dto;
mod feign;
mod log;
mod svc;
mod vo;
mod web;

use crate::dao::{dao_macro, DaoArgs};
use crate::db::MigrateArgs;
use crate::dto::{crud_dto_macro, CrudDtoArgs};
use crate::feign::feign_macro;
use crate::log::{log_call_macro, LogCallArgs};
use crate::svc::{db_unwrap_macro, svc_macro, DbUnwrapArgs};
use crate::vo::{vo_macro, VoArgs};
use crate::web::{api_doc_macro, ctrl_macro, router_macro, ApiDocArgs, RouterArgs};
use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput, ItemFn, ItemStruct};

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

/// 过程宏：在异步函数中生成数据库迁移代码
///
/// 调用时需要传入一个保存数据库连接地址字符串的变量名（支持 MySQL、PostgreSQL、
/// SQLite），宏会在调用位置展开为连接数据库并执行迁移的代码，并根据数据库类型
/// 自动选择对应的迁移目录（`migrations/mysql`、`migrations/pgsql`、`migrations/sqlite`）。
///
/// # 使用示例
/// ```
/// async fn migrate_db(db_url: String) -> anyhow::Result<()> {
///     db_migrate!(db_url);
///     Ok(())
/// }
/// ```
#[proc_macro]
pub fn db_migrate(args: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as MigrateArgs);
    db::db_migrate_macro(args).into()
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
pub fn crud_dto(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as CrudDtoArgs);
    let input = parse_macro_input!(input as ItemStruct);
    crud_dto_macro(args, input).into()
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
pub fn vo(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as VoArgs);
    let input = parse_macro_input!(input as DeriveInput);
    vo_macro(args, input).into()
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

/// 属性宏：为以 `Svc` 结尾的结构体自动生成标准 CRUD 方法
///
/// 该宏会解析结构体名称（必须是 `XxxSvc` 形式的大驼峰命名），为结构体生成
/// `add`、`modify`、`save`、`del_by_id`、`del_by_query_dto`、`get_by_id`、
/// `get_by_query_dto`、`list_by_query_dto`、`page_by_query_dto` 以及对应的
/// Ex 版本（附带关联表信息）等方法。生成的方法内部调用对应的 `XxxDao`，
/// 返回 `Ro<Vo>`、`Ro<ExVo>`、`Ro<PageRx<Vo>>` 等统一响应格式。
///
/// 使用前提：项目中需存在与结构体名称对应的 `dto`、`dao`、`mo`、`vo` 模块。
///
/// # 使用示例
/// ```
/// #[svc]
/// pub struct OssBucketSvc;
/// ```
#[proc_macro_attribute]
pub fn svc(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    svc_macro(input).into()
}

/// 属性宏：为以 `Ctrl` 结尾的结构体自动生成标准 CRUD Web 处理器
///
/// 该宏会解析结构体名称（必须是 `XxxCtrl` 形式的大驼峰命名），为结构体生成
/// 一组 axum handler 方法（`add`、`modify`、`save`、`del_by_id`、`get_by_id`、
/// `get_by_query_dto`、`list_by_query_dto`、`page_by_query_dto` 及 Ex 版本），
/// 每个方法带有 `#[utoipa::path]`、`#[debug_handler]`、`#[log_call]` 属性，
/// 内部调用对应的 `XxxSvc` 并返回 `Json<Ro<...>>` 格式响应。
///
/// 使用前提：项目中需存在与结构体名称对应的 `dto`、`svc`、`vo` 模块。
///
/// # 使用示例
/// ```
/// #[ctrl]
/// pub struct OssBucketCtrl;
/// ```
#[proc_macro_attribute]
pub fn ctrl(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    ctrl_macro(input).into()
}

/// 属性宏：为以 `Router` 结尾的结构体生成路由注册函数
///
/// 该宏会解析结构体名称（必须是 `XxxRouter` 形式的大驼峰命名），生成
/// `build_router` 函数，并通过 `linkme::distributed_slice` 注册到
/// `robotech::web::ROUTER_SLICE` 全局路由切片，实现路由的分布式收集。
///
/// 支持以下参数：
/// - `crud`：自动注册基于 `XxxCtrl` 的完整 CRUD 路由（增删改查、分页、Ex 等）
/// - `routes = [(path, handler), ...]`：自定义路由列表
///
/// # 使用示例
/// ```
/// #[router(crud, routes = [("/hello", get(hello))])]
/// pub struct OssBucketRouter;
/// ```
#[proc_macro_attribute]
pub fn router(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as RouterArgs);
    let input = parse_macro_input!(input as ItemStruct);
    router_macro(args, input).into()
}

/// 属性宏：为以 `ApiDoc` 结尾的结构体生成 OpenAPI 文档聚合
///
/// 该宏会解析结构体名称（必须是 `XxxApiDoc` 形式的大驼峰命名），生成一个
/// 派生 `utoipa::OpenApi` 的文档结构体，并通过 `linkme::distributed_slice`
/// 注册到 `robotech::web::API_DOC_SLICE`，文档地址为 `/xxx/yyy/openapi.json`。
///
/// 宏参数为逗号分隔的 OpenAPI path 标识符列表（对应 `XxxCtrl` 中由
/// `#[utoipa::path]` 标注的接口函数名），不可省略。
///
/// # 使用示例
/// ```
/// #[api_doc(add, modify, get_by_id, page_by_query_dto)]
/// pub struct OssBucketApiDoc;
/// ```
#[proc_macro_attribute]
pub fn api_doc(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as ApiDocArgs);
    let input = parse_macro_input!(input as ItemStruct);
    api_doc_macro(args, input).into()
}

/// 属性宏：为 Feign API 客户端结构体自动生成 `build_headers` 辅助方法
///
/// 该宏会为包装了 `FeignApiClient` 的结构体生成一个 `build_headers` 方法，
/// 用于构建包含当前用户 ID 的请求头，避免在每个方法中重复编写 header 构建逻辑。
///
/// # 使用示例
/// ```
/// use robotech::macros::feign_client;
///
/// #[feign_client]
/// pub struct OssFileApiClient {
///     client: FeignApiClient,
/// }
/// ```
///
/// 展开后会生成：
/// ```
/// impl OssFileApiClient {
///     fn build_headers(
///         current_user_id: u64,
///     ) -> Result<HeaderMap, ApiClientError> {
///         // ... 构建包含 USER_ID_HEADER_NAME 的 HeaderMap
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn feign(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    feign_macro(input).into()
}