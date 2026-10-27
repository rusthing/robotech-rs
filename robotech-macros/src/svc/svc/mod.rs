//! # svc 属性宏
//!
//! 实现 `#[svc]` 属性宏的代码生成，为以 `Svc` 结尾的结构体生成标准的
//! 增删改查、分页查询及关联表查询（Ex 系列）Service 方法。

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::{HashMap, HashSet};
use syn::{
    bracketed,
    parse::{Parse, ParseStream},
    Ident, ItemStruct, Token,
};
use wheel_rs::str_utils::{split_camel_case, CamelFormat};

/// 写操作方法名集合（before_write / after_write 快捷方式生效范围）
const WRITE_METHODS: &[&str] = &["add", "modify", "del_by_id", "del_by_query_dto"];

/// `#[svc]` 宏参数
pub(crate) struct SvcArgs {
    /// 需要跳过的的方法名集合
    pub skip: HashSet<String>,
    /// 写操作前的回调（add / modify / del_by_id / del_by_query_dto）
    pub before_write: Option<syn::Path>,
    /// 写操作后的回调（add / modify / del_by_id / del_by_query_dto）
    pub after_write: Option<syn::Path>,
    /// 每个方法单独的前置回调：方法名 → 函数路径
    pub before: HashMap<String, syn::Path>,
    /// 每个方法单独的后置回调：方法名 → 函数路径
    pub after: HashMap<String, syn::Path>,
}

impl Parse for SvcArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut skip = HashSet::new();
        let mut before_write = None;
        let mut after_write = None;
        let mut before = HashMap::new();
        let mut after = HashMap::new();

        if input.is_empty() {
            return Ok(SvcArgs {
                skip,
                before_write,
                after_write,
                before,
                after,
            });
        }

        loop {
            let ident: Ident = input.parse()?;
            let ident_str = ident.to_string();
            match ident_str.as_str() {
                "skip" => {
                    let lookahead = input.lookahead1();
                    if lookahead.peek(Token![:]) {
                        let _: Token![:] = input.parse()?;
                    } else if lookahead.peek(Token![=]) {
                        let _: Token![=] = input.parse()?;
                    }

                    let content;
                    bracketed!(content in input);
                    let method_names =
                        content.parse_terminated(Ident::parse, Token![,])?;
                    for method_name in method_names {
                        skip.insert(method_name.to_string());
                    }
                }
                "before_write" => {
                    let _: Token![=] = input.parse()?;
                    before_write = Some(input.parse()?);
                }
                "after_write" => {
                    let _: Token![=] = input.parse()?;
                    after_write = Some(input.parse()?);
                }
                name if name.starts_with("before_") && name.len() > "before_".len() => {
                    let method = name["before_".len()..].to_string();
                    let _: Token![=] = input.parse()?;
                    before.insert(method, input.parse()?);
                }
                name if name.starts_with("after_") && name.len() > "after_".len() => {
                    let method = name["after_".len()..].to_string();
                    let _: Token![=] = input.parse()?;
                    after.insert(method, input.parse()?);
                }
                unknown => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        format!("Unknown argument: {unknown}"),
                    ));
                }
            }

            if input.is_empty() {
                break;
            }
            let _: Token![,] = input.parse()?;
        }

        Ok(SvcArgs {
            skip,
            before_write,
            after_write,
            before,
            after,
        })
    }
}

/// 为指定方法解析钩子函数调用
///
/// 优先级：单方法钩子 > 写操作快捷方式（仅写方法生效）。
///
/// ## 参数传递与变量遮蔽
///
/// `pass_var` 为返回值变量（会被钩子返回值遮蔽，允许钩子修改核心数据）。
///
/// `pass_input` 为额外的输入参数，仅对 `after_<method>` 单方法钩子生效，
/// 传入后钩子签名为 `fn(input, output, db) -> Result<output, E>`。
///
/// `with_db` 为 `true` 时把当前连接/事务作为**最后一个参数**交给钩子，
/// 钩子因此能读到「本方法还没提交的写」——级联写（改渠道编码顺带改依赖它的
/// 配置）必须如此：钩子与主写要在同一个事务里，才能一起提交、一起回滚。
///
/// ```ignore
/// // before_xxx 钩子（无 pass_input）
/// let var = hook_fn(var, db).await?;
///
/// // after_xxx 钩子（有 pass_input）
/// let output = hook_fn(input, output, db).await?;
/// ```
///
/// `with_db = false` 只用于 `save`：它的 `db` 还是 `Option<&C>`（没经过
/// `#[db_unwrap]` 展开），钩子拿到的 `db` 形态不一致。
///
/// `before_write` / `after_write` 快捷方式始终为无参副作用调用：它们被多个
/// 服务复用，加参数要同步改所有签名。
fn resolve_hook(
    method_name: &str,
    is_write: bool,
    per_method: &HashMap<String, syn::Path>,
    fallback: &Option<syn::Path>,
    pass_var: Option<TokenStream>,
    pass_input: Option<TokenStream>,
    with_db: bool,
) -> Option<TokenStream> {
    // 带前导逗号，直接拼在已有参数后面；不带 db 时展开为空
    let db_arg: TokenStream = if with_db {
        quote! { , db }
    } else {
        quote! {}
    };
    if let Some(fn_path) = per_method.get(method_name) {
        if let Some(var) = pass_var {
            if let Some(input) = pass_input {
                return Some(quote! { let #var = #fn_path(#input, #var #db_arg).await?; });
            }
            return Some(quote! { let #var = #fn_path(#var #db_arg).await?; });
        }
        if with_db {
            return Some(quote! { let _ = #fn_path(db).await; });
        }
        return Some(quote! { let _ = #fn_path().await; });
    }
    if is_write {
        fallback.as_ref().map(|f| quote! { let _ = #f().await; })
    } else {
        None
    }
}

/// `after_write` 快捷方式的通知调用
///
/// 只在方法没有自己的 `after_<method>` 时生成，与 [`resolve_hook`] 的
/// 「单方法钩子优先于快捷方式」保持一致，避免两者叠加触发两次。
///
/// 这段代码放在写方法的**事务提交之后**：刷新信号若在提交前发出，收到信号的
/// 节点会读到未提交的旧数据并缓存下来，而信号已经消费掉，缓存就永久是旧的。
fn write_notify(args: &SvcArgs, method_name: &str) -> TokenStream {
    if args.after.contains_key(method_name) {
        return quote! {};
    }
    match &args.after_write {
        Some(f) => quote! { let _ = #f().await; },
        None => quote! {},
    }
}

pub(crate) fn svc_macro(args: SvcArgs, input: ItemStruct) -> TokenStream {
    let skip = &args.skip;
    let struct_name = &input.ident;

    // 解析结构体的名称，必须是Svc结尾，符合大驼峰命名规范
    let struct_name_str = struct_name.to_string();
    if !struct_name_str.ends_with("Svc") {
        return syn::Error::new_spanned(struct_name, "Struct name must end with 'Svc'")
            .to_compile_error()
            .into();
    }
    let struct_name_split = split_camel_case(&struct_name_str, CamelFormat::Upper);
    if struct_name_split.is_err() {
        return syn::Error::new_spanned(
            struct_name,
            "Struct name must be a valid upper camel case",
        )
        .to_compile_error()
        .into();
    }
    let mut struct_name_split = struct_name_split.unwrap();
    struct_name_split.pop();
    let module_name = struct_name_split.join("_").to_lowercase();
    let module = format_ident!("{module_name}");
    let dto_module = format_ident!("{module_name}_dto");
    let entity_name = struct_name_split.join("");
    let dao_name = format_ident!("{}Dao", entity_name);
    let vo_name = format_ident!("{}Vo", entity_name);
    let ex_vo_name = format_ident!("{}ExVo", entity_name);
    let add_dto_name = format_ident!("{}AddDto", entity_name);
    let modify_dto_name = format_ident!("{}ModifyDto", entity_name);
    let save_dto_name = format_ident!("{}SaveDto", entity_name);
    let query_dto_name = format_ident!("{}QueryDto", entity_name);

    let mut generated_methods = Vec::new();

    // 生成add方法
    if !skip.contains("add") {
        let is_write = WRITE_METHODS.contains(&"add");
        let before_call = resolve_hook("add", is_write, &args.before, &args.before_write, Some(quote! { add_dto }), None, true);
        // 事务内的钩子只走单方法 after_add；after_write 快捷方式改在提交后调用
        let after_call = resolve_hook("add", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { add_dto }), true);
        let after_commit = write_notify(&args, "add");
        generated_methods.push(quote! {
        /// # 添加新记录
        ///
        /// 将提供的AddTo对象转换为ActiveModel并插入到数据库中
        ///
        /// 未传 `db` 时会自开事务：`before_add` / `after_add` 钩子与本次插入
        /// 同属一个事务，提交成功后才执行 `after_write` 通知。
        ///
        /// ## 参数
        /// * `add_to` - 包含要添加记录信息的传输对象
        /// * `db` - 数据库连接或事务，如果未提供则创建连接及事务
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vo>)` - 添加成功，返回封装了新增Vo的Ro对象
        /// * `Err(SvcError)` - 添加失败，可能是因为违反唯一约束或其他数据库错误
        #[log_call]
        pub async fn add<C>(
            add_dto: #add_dto_name,
            #[skip_log]
            db: Option<&C>,
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            // 调用方带了连接：事务归调用方，这里只写不提交
            if let Some(db) = db {
                #before_call
                // 先校验dto
                add_dto.validate()?;

                let active_model: ActiveModel = add_dto.clone().into();
                let one = #vo_name::from(#dao_name::insert(active_model, db).await?);
                #after_call
                // 调用方的事务还没提交，通知只能先发；需要「提交后再通知」的
                // 调用方，应传 None 让本方法自己开事务
                #after_commit
                return Ok(Ro::success("添加成功".to_string()).extra(Some(one)));
            }

            // 没带连接：自己开事务。块内 `?` 与 `return` 会直接退出函数，
            // tx 随之 drop 即回滚，只有正常走到块尾才提交
            let db_conn = robotech::db::get_db_conn()?;
            let tx = begin_transaction(db_conn.as_ref()).await?;
            let one = {
                let db = &tx;
                #before_call
                // 先校验dto
                add_dto.validate()?;

                let active_model: ActiveModel = add_dto.clone().into();
                let one = #vo_name::from(#dao_name::insert(active_model, db).await?);
                #after_call
                one
            };
            commit_transaction(tx).await?;
            #after_commit
            Ok(Ro::success("添加成功".to_string()).extra(Some(one)))
        }
        });
    }

    // 生成modify方法
    if !skip.contains("modify") {
        let is_write = WRITE_METHODS.contains(&"modify");
        let before_call = resolve_hook("modify", is_write, &args.before, &args.before_write, Some(quote! { modify_dto }), None, true);
        let after_call = resolve_hook("modify", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { modify_dto }), true);
        let after_commit = write_notify(&args, "modify");
        generated_methods.push(quote! {
        /// # 修改记录
        ///
        /// 根据提供的ModifyTo对象更新数据库中的相应记录
        ///
        /// 未传 `db` 时会自开事务：`before_modify` / `after_modify` 钩子与本
        /// 次更新同属一个事务，提交成功后才执行 `after_write` 通知。
        ///
        /// ## 参数
        /// * `modify_to` - 包含要修改记录信息的传输对象，必须包含有效的ID
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vo>)` - 修改成功，返回封装了更新后Vo的Ro对象
        /// * `Err(SvcError)` - 修改失败，可能因为记录不存在、违反唯一约束或其他数据库错误
        #[log_call]
        pub async fn modify<C>(
            modify_dto: #modify_dto_name,
            #[skip_log]
            db: Option<&C>,
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            // 调用方带了连接：事务归调用方，这里只写不提交
            if let Some(db) = db {
                #before_call
                // 先校验dto
                modify_dto.validate()?;

                let active_model: ActiveModel = modify_dto.clone().into();
                let one = #vo_name::from(#dao_name::update(active_model, db).await?);
                #after_call
                #after_commit
                return Ok(Ro::success("修改成功".to_string()).extra(Some(one)));
            }

            // 没带连接：自己开事务，块内 `?` / `return` 会让 tx drop 回滚
            let db_conn = robotech::db::get_db_conn()?;
            let tx = begin_transaction(db_conn.as_ref()).await?;
            let one = {
                let db = &tx;
                #before_call
                // 先校验dto
                modify_dto.validate()?;

                let active_model: ActiveModel = modify_dto.clone().into();
                let one = #vo_name::from(#dao_name::update(active_model, db).await?);
                #after_call
                one
            };
            commit_transaction(tx).await?;
            #after_commit
            Ok(Ro::success("修改成功".to_string()).extra(Some(one)))
        }
        });
    }

    // 生成save方法
    if !skip.contains("save") {
        let before_call = resolve_hook("save", false, &args.before, &args.before_write, Some(quote! { save_dto }), None, false);
        let after_call = resolve_hook("save", false, &args.after, &args.after_write, None, None, false);
        generated_methods.push(quote! {
        /// # 保存记录
        ///
        /// 根据提供的SaveTo对象保存记录到数据库中。如果提供了ID，则更新现有记录；如果没有提供ID，则创建新记录
        ///
        /// ## 参数
        /// * `save_to` - 包含要保存记录信息的传输对象
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vo>)` - 保存成功，返回封装了Vo的Ro对象
        /// * `Err(SvcError)` - 保存失败，可能因为违反唯一约束、记录不存在或其他数据库错误
        pub async fn save<C>(
            save_dto: #save_dto_name,
            db: Option<&C>,
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let result = if let Some(id) = save_dto.id {
                Self::modify(save_dto.clone().into(), db).await
            } else {
                Self::add(save_dto.clone().into(), db).await
            };
            #after_call
            result
        }
        });
    }

    // 生成del_by_id方法
    if !skip.contains("del_by_id") {
        let is_write = WRITE_METHODS.contains(&"del_by_id");
        let before_call = resolve_hook("del_by_id", is_write, &args.before, &args.before_write, Some(quote! { id }), None, true);
        let after_call = resolve_hook("del_by_id", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { id }), true);
        let after_commit = write_notify(&args, "del_by_id");
        generated_methods.push(quote! {
        /// # 删除记录
        ///
        /// 根据提供的ID删除数据库中的相应记录
        ///
        /// 未传 `db` 时会自开事务：查询待删记录、删除本身、后续钩子都在同一
        /// 事务里，删除成功提交后才执行 `after_write` 通知。
        ///
        /// ## 参数
        /// * `id` - 要删除的记录的ID
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vo>)` - 删除成功，返回封装了Vo的Ro对象
        /// * `Err(SvcError)` - 删除失败，可能因为记录不存在或其他数据库错误
        #[log_call]
        pub async fn del_by_id<C>(
            id: U64,
            #[skip_log]
            db: Option<&C>,
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            // 调用方带了连接：事务归调用方，这里只写不提交
            if let Some(db) = db {
                #before_call
                let one = Self::get_by_id(id, Some(db))
                    .await?
                    .extra
                    .ok_or(SvcError::NotFound(id.to_string()))?;
                let rows_affected = #dao_name::delete(
                    ActiveModel {
                        id: sea_orm::ActiveValue::Set(id.into()),
                        ..Default::default()
                    },
                    db,
                )
                .await?.rows_affected;
                if rows_affected == 0 {
                    return Err(SvcError::NotFound(id.to_string()));
                }
                #after_call
                #after_commit
                return Ok(Ro::success("删除成功".to_string()).extra(Some(one)));
            }

            // 没带连接：自己开事务，块内 `?` / `return` 会让 tx drop 回滚
            let db_conn = robotech::db::get_db_conn()?;
            let tx = begin_transaction(db_conn.as_ref()).await?;
            let one = {
                let db = &tx;
                #before_call
                let one = Self::get_by_id(id, Some(db))
                    .await?
                    .extra
                    .ok_or(SvcError::NotFound(id.to_string()))?;
                let rows_affected = #dao_name::delete(
                    ActiveModel {
                        id: sea_orm::ActiveValue::Set(id.into()),
                        ..Default::default()
                    },
                    db,
                )
                .await?.rows_affected;
                if rows_affected == 0 {
                    return Err(SvcError::NotFound(id.to_string()));
                }
                #after_call
                one
            };
            commit_transaction(tx).await?;
            #after_commit
            Ok(Ro::success("删除成功".to_string()).extra(Some(one)))
        }
        });
    }

    // 生成del_by_query_dto方法
    if !skip.contains("del_by_query_dto") {
        let is_write = WRITE_METHODS.contains(&"del_by_query_dto");
        let before_call = resolve_hook("del_by_query_dto", is_write, &args.before, &args.before_write, Some(quote! { dto }), None, true);
        let after_call = resolve_hook("del_by_query_dto", false, &args.after, &args.after_write, Some(quote! { rows_affected }), Some(quote! { dto }), true);
        let after_commit = write_notify(&args, "del_by_query_dto");
        generated_methods.push(quote! {
        /// # 删除记录
        ///
        /// 根据提供的查询参数获取数据库中的记录
        ///
        /// 未传 `db` 时会自开事务，删除成功提交后才执行 `after_write` 通知。
        ///
        /// ## 参数
        /// * `dto` - 查询参数
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Result<Ro<Vo>, SvcError>` - 查询结果封装为Ro对象，如果查询成功则返回封装了Vo的Ro对象，否则返回错误信息
        #[log_call]
        pub async fn del_by_query_dto<C>(
            dto: #query_dto_name,
            #[skip_log]
            db: Option<&C>,
        ) -> Result<Ro<()>, SvcError>
        where
            C: ConnectionTrait,
        {
            // 调用方带了连接：事务归调用方，这里只写不提交
            if let Some(db) = db {
                #before_call
                let mut condition = dto.to_condition();
                if let Some(keyword) = &dto._keyword {
                    condition = condition.add(build_like_condition(keyword, #dao_name::LIKE_COLUMNS));
                }

                let rows_affected = #dao_name::delete_by_condition(condition, db).await?.rows_affected;
                if rows_affected == 0 {
                    return Err(SvcError::NotFound(dto.to_string()));
                }
                #after_call
                #after_commit
                return Ok(Ro::success(format!("删除了{}条记录", rows_affected).to_string()));
            }

            // 没带连接：自己开事务，块内 `?` / `return` 会让 tx drop 回滚
            let db_conn = robotech::db::get_db_conn()?;
            let tx = begin_transaction(db_conn.as_ref()).await?;
            let rows_affected = {
                let db = &tx;
                #before_call
                let mut condition = dto.to_condition();
                if let Some(keyword) = &dto._keyword {
                    condition = condition.add(build_like_condition(keyword, #dao_name::LIKE_COLUMNS));
                }

                let rows_affected = #dao_name::delete_by_condition(condition, db).await?.rows_affected;
                if rows_affected == 0 {
                    return Err(SvcError::NotFound(dto.to_string()));
                }
                #after_call
                rows_affected
            };
            commit_transaction(tx).await?;
            #after_commit
            Ok(Ro::success(format!("删除了{}条记录", rows_affected).to_string()))
        }
        });
    }

    // 生成get_by_id方法
    if !skip.contains("get_by_id") {
        let before_call = resolve_hook("get_by_id", false, &args.before, &args.before_write, Some(quote! { id }), None, true);
        let after_call = resolve_hook("get_by_id", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { id }), true);
        generated_methods.push(quote! {
        /// # 根据id获取记录信息
        ///
        /// 通过提供的ID从数据库中查询相应的记录，如果找到则返回封装了Vo的Ro对象，否则返回对象的extra为None
        ///
        /// ## 参数
        /// * `id` - 要查询的桶的ID
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vo>)` - 查询成功，如果记录存在，返回封装了Vo的Ro对象，如果不存在则返回对象的extra为None
        /// * `Err(SvcError)` - 查询失败，可能是数据库错误
        #[db_unwrap]
        #[log_call]
        pub async fn get_by_id<C>(
            id: U64,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let one = #dao_name::get_by_id::<_, #vo_name>(id, db).await?;
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(one))
        }
        });
    }

    // 生成get_by_query_dto方法
    if !skip.contains("get_by_query_dto") {
        let before_call = resolve_hook("get_by_query_dto", false, &args.before, &args.before_write, Some(quote! { dto }), None, true);
        let after_call = resolve_hook("get_by_query_dto", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { dto }), true);
        generated_methods.push(quote! {
        /// # 获取记录
        ///
        /// 根据提供的查询参数获取数据库中的记录
        ///
        /// ## 参数
        /// * `dto` - 查询参数
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Result<Ro<Vo>, SvcError>` - 查询结果封装为Ro对象，如果查询成功则返回封装了Vo的Ro对象，否则返回错误信息
        #[db_unwrap]
        #[log_call]
        pub async fn get_by_query_dto<C>(
            dto: #query_dto_name,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let mut condition = dto.to_condition();
            if let Some(keyword) = &dto._keyword {
                condition = condition.add(build_like_condition(keyword, #dao_name::LIKE_COLUMNS));
            }

            let one = #dao_name::get_by_condition::<_, #vo_name>(condition, db).await?;
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(one))
        }
        });
    }

    // 生成list_by_query_dto方法
    if !skip.contains("list_by_query_dto") {
        let before_call = resolve_hook("list_by_query_dto", false, &args.before, &args.before_write, Some(quote! { dto }), None, true);
        let after_call = resolve_hook("list_by_query_dto", false, &args.after, &args.after_write, Some(quote! { all }), Some(quote! { dto }), true);
        generated_methods.push(quote! {
        /// # 查询记录列表
        ///
        /// 根据提供的查询参数获取数据库中的记录列表
        ///
        /// ## 参数
        /// * `dto` - 查询参数
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Result<Ro<Vec<Vo>>, SvcError>` - 查询结果封装为Ro对象，如果查询成功则返回封装了Vo的Ro对象，否则返回错误信息
        #[db_unwrap]
        #[log_call]
        pub async fn list_by_query_dto<C>(
            dto: #query_dto_name,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<Vec<#vo_name>>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let keyword = &dto._keyword;
            let order_by = &dto._order_by;
            let mut condition = dto.to_condition();
            if let Some(keyword) = keyword {
                condition = condition.add(build_like_condition(keyword, #dao_name::LIKE_COLUMNS));
            }

            let all = #dao_name::list_by_condition::<_, #vo_name>(condition, order_by, db).await?;
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(Some(all)))
        }
        });
    }

    // 生成page_by_query_dto方法
    if !skip.contains("page_by_query_dto") {
        let before_call = resolve_hook("page_by_query_dto", false, &args.before, &args.before_write, Some(quote! { dto }), None, true);
        let after_call = resolve_hook("page_by_query_dto", false, &args.after, &args.after_write, Some(quote! { models }), Some(quote! { dto }), true);
        generated_methods.push(quote! {
        /// # 查询记录列表
        ///
        /// 根据提供的查询参数获取数据库中的记录列表
        ///
        /// ## 参数
        /// * `dto` - 查询参数
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Result<Ro<Vec<Vo>>, SvcError>` - 查询结果封装为Ro对象，如果查询成功则返回封装了Vo的Ro对象，否则返回错误信息
        #[db_unwrap]
        #[log_call]
        pub async fn page_by_query_dto<C>(
            dto: #query_dto_name,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<PageRx<#vo_name>>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let keyword = &dto._keyword;
            let order_by = &dto._order_by;
            let page_num = dto._page.unwrap_or(U64(1));
            let page_size = dto._size.unwrap_or(U64(10));

            let mut condition = dto.to_condition();
            if let Some(keyword) = keyword {
                condition = condition.add(build_like_condition(keyword, #dao_name::LIKE_COLUMNS));
            }

            let (page_num, total, models) = #dao_name::page_by_condition::<_, #vo_name>(
                condition,
                order_by,
                page_num,
                page_size,
                db
            ).await?;
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(Some(PageRx::builder()
                .total(total)
                .page_num(page_num)
                .list(models)
                .build()
            )))
        }
        });
    }

    // 生成get_ex_by_id方法
    if !skip.contains("get_ex_by_id") {
        let before_call = resolve_hook("get_ex_by_id", false, &args.before, &args.before_write, Some(quote! { id }), None, true);
        let after_call = resolve_hook("get_ex_by_id", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { id }), true);
        generated_methods.push(quote! {
        /// # 根据id获取记录信息(附带获取关联表的信息)
        ///
        /// 通过提供的ID从数据库中查询相应的记录，如果找到则返回封装了ExVo的Ro对象，否则返回对象的extra为None
        ///
        /// ## 参数
        /// * `id` - 要查询的桶的ID
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<ExVo>)` - 查询成功，如果记录存在，返回封装了Vo的Ro对象，如果不存在则返回对象的extra为None
        /// * `Err(SvcError)` - 查询失败，可能是数据库错误
        #[db_unwrap]
        #[log_call]
        pub async fn get_ex_by_id<C>(
            id: U64,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<#ex_vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let one: Option<#ex_vo_name> = #dao_name::get_ex_by_id(id, db)
                .await?
                .map(|m| m.into());
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(one))
        }
        });
    }

    // 生成get_ex_by_query_dto方法
    if !skip.contains("get_ex_by_query_dto") {
        let before_call = resolve_hook("get_ex_by_query_dto", false, &args.before, &args.before_write, Some(quote! { dto }), None, true);
        let after_call = resolve_hook("get_ex_by_query_dto", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { dto }), true);
        generated_methods.push(quote! {
        /// # 获取记录信息(附带获取关联表的信息)
        ///
        /// 根据提供的查询参数获取数据库中的记录
        ///
        /// ## 参数
        /// * `dto` - 查询参数
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Result<Ro<ExVo>, SvcError>` - 查询结果封装为Ro对象，如果查询成功则返回封装了ExVo的Ro对象，否则返回错误信息
        #[db_unwrap]
        #[log_call]
        pub async fn get_ex_by_query_dto<C>(
            dto: #query_dto_name,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<#ex_vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let mut condition = dto.to_condition();
            if let Some(keyword) = &dto._keyword {
                condition = condition.add(build_like_condition(keyword, #dao_name::LIKE_COLUMNS));
            }

            let one: Option<#ex_vo_name> = #dao_name::get_ex_by_condition(condition, db)
                .await?
                .map(|m| m.into());
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(one))
        }
        });
    }

    // 生成list_ex_by_query_dto方法
    if !skip.contains("list_ex_by_query_dto") {
        let before_call = resolve_hook("list_ex_by_query_dto", false, &args.before, &args.before_write, Some(quote! { dto }), None, true);
        let after_call = resolve_hook("list_ex_by_query_dto", false, &args.after, &args.after_write, Some(quote! { all }), Some(quote! { dto }), true);
        generated_methods.push(quote! {
        /// # 查询记录列表(附带获取关联表的信息)
        ///
        /// 根据提供的查询参数获取数据库中的记录列表
        ///
        /// ## 参数
        /// * `dto` - 查询参数
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Result<Ro<Vec<ExVo>>, SvcError>` - 查询结果封装为Ro对象，如果查询成功则返回封装了ExVo的Ro对象，否则返回错误信息
        #[db_unwrap]
        #[log_call]
        pub async fn list_ex_by_query_dto<C>(
            dto: #query_dto_name,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<Vec<#ex_vo_name>>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let keyword = &dto._keyword;
            let order_by = &dto._order_by;
            let mut condition = dto.to_condition();
            if let Some(keyword) = keyword {
                condition = condition.add(build_like_condition(keyword, #dao_name::LIKE_COLUMNS));
            }

            let all: Vec<#ex_vo_name> = #dao_name::list_ex_by_condition(condition, order_by, db)
                .await?
                .into_iter()
                .map(|m| m.into())
                .collect();
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(Some(all)))
        }
        });
    }

    // 生成page_ex_by_query_dto方法
    if !skip.contains("page_ex_by_query_dto") {
        let before_call = resolve_hook("page_ex_by_query_dto", false, &args.before, &args.before_write, Some(quote! { dto }), None, true);
        let after_call = resolve_hook("page_ex_by_query_dto", false, &args.after, &args.after_write, Some(quote! { list }), Some(quote! { dto }), true);
        generated_methods.push(quote! {
        /// # 查询记录列表(附带获取关联表的信息)
        ///
        /// 根据提供的查询参数获取数据库中的记录列表
        ///
        /// ## 参数
        /// * `dto` - 查询参数
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Result<Ro<PageRx<ExVo>>, SvcError>` - 查询结果封装为Ro对象，如果查询成功则返回封装了ExVo的Ro对象，否则返回错误信息
        #[db_unwrap]
        #[log_call]
        pub async fn page_ex_by_query_dto<C>(
            dto: #query_dto_name,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<PageRx<#ex_vo_name>>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let keyword = &dto._keyword;
            let order_by = &dto._order_by;
            let page_num = dto._page.unwrap_or(U64(1));
            let page_size = dto._size.unwrap_or(U64(10));

            let mut condition = dto.to_condition();
            if let Some(keyword) = keyword {
                condition = condition.add(build_like_condition(keyword, #dao_name::LIKE_COLUMNS));
            }

            let (page_num, total, models) = #dao_name::page_ex_by_condition(
                condition,
                order_by,
                page_num,
                page_size,
                db
            ).await?;
            let list: Vec<#ex_vo_name> = models.into_iter().map(|m| m.into()).collect();
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(Some(PageRx::builder()
                .total(total)
                .page_num(page_num)
                .list(list)
                .build()
            )))
        }
        });
    }

    // ==================== Bean Searcher 兼容检索方法 ====================

    // 生成search_by_id方法
    if !skip.contains("search_by_id") {
        let before_call = resolve_hook("search_by_id", false, &args.before, &args.before_write, Some(quote! { params }), None, true);
        let after_call = resolve_hook("search_by_id", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { params }), true);
        generated_methods.push(quote! {
        /// # 按 ID 检索单条记录
        ///
        /// 在 `id` 条件之上叠加检索参数中的附加条件，返回单条记录
        ///
        /// ## 参数
        /// * `id` - 要查询的记录 ID
        /// * `params` - 检索参数（字段条件按 Bean Searcher 规则，全局参数用原生 `_` 前缀）
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vo>)` - 查询成功，记录不存在时 `extra` 为 `None`
        /// * `Err(SvcError)` - 查询失败
        #[db_unwrap]
        #[log_call]
        pub async fn search_by_id<C>(
            id: U64,
            params: SearchParams,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let one = #dao_name::search_by_id::<_, #vo_name>(id, &params, db).await?;
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(one))
        }
        });
    }

    // 生成search_by_condition方法
    if !skip.contains("search_by_condition") {
        let before_call = resolve_hook("search_by_condition", false, &args.before, &args.before_write, Some(quote! { params }), None, true);
        let after_call = resolve_hook("search_by_condition", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { params }), true);
        generated_methods.push(quote! {
        /// # 按检索参数获取单条记录
        ///
        /// 根据检索参数查询数据库中的记录
        ///
        /// ## 参数
        /// * `params` - 检索参数（字段条件按 Bean Searcher 规则，全局参数用原生 `_` 前缀）
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vo>)` - 查询成功，记录不存在时 `extra` 为 `None`
        /// * `Err(SvcError)` - 查询失败
        #[db_unwrap]
        #[log_call]
        pub async fn search_by_condition<C>(
            params: SearchParams,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let one = #dao_name::search_by_condition::<_, #vo_name>(&params, db).await?;
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(one))
        }
        });
    }

    // 生成search_list方法
    if !skip.contains("search_list") {
        let before_call = resolve_hook("search_list", false, &args.before, &args.before_write, Some(quote! { params }), None, true);
        let after_call = resolve_hook("search_list", false, &args.after, &args.after_write, Some(quote! { all }), Some(quote! { params }), true);
        generated_methods.push(quote! {
        /// # 按检索参数查询记录列表
        ///
        /// 根据检索参数查询数据库中的记录列表，支持排序
        ///
        /// ## 参数
        /// * `params` - 检索参数（字段条件按 Bean Searcher 规则，全局参数用原生 `_` 前缀）
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vec<Vo>>)` - 查询成功，返回记录列表
        /// * `Err(SvcError)` - 查询失败
        #[db_unwrap]
        #[log_call]
        pub async fn search_list<C>(
            params: SearchParams,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<Vec<#vo_name>>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let all = #dao_name::search_list::<_, #vo_name>(&params, db).await?;
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(Some(all)))
        }
        });
    }

    // 生成search_page方法
    if !skip.contains("search_page") {
        let before_call = resolve_hook("search_page", false, &args.before, &args.before_write, Some(quote! { params }), None, true);
        let after_call = resolve_hook("search_page", false, &args.after, &args.after_write, Some(quote! { models }), Some(quote! { params }), true);
        generated_methods.push(quote! {
        /// # 按检索参数分页查询记录列表
        ///
        /// 根据检索参数分页查询数据库中的记录列表，支持排序
        ///
        /// ## 参数
        /// * `params` - 检索参数（含分页与排序；字段条件按 Bean Searcher 规则，全局参数用原生 `_` 前缀）
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<PageRx<Vo>>)` - 查询成功，返回分页结果
        /// * `Err(SvcError)` - 查询失败
        #[db_unwrap]
        #[log_call]
        pub async fn search_page<C>(
            params: SearchParams,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<PageRx<#vo_name>>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let (page_num, total, models) =
                #dao_name::search_page::<_, #vo_name>(&params, db).await?;
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(Some(PageRx::builder()
                .total(total)
                .page_num(page_num)
                .list(models)
                .build()
            )))
        }
        });
    }

    // 生成search_ex_by_id方法
    if !skip.contains("search_ex_by_id") {
        let before_call = resolve_hook("search_ex_by_id", false, &args.before, &args.before_write, Some(quote! { params }), None, true);
        let after_call = resolve_hook("search_ex_by_id", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { params }), true);
        generated_methods.push(quote! {
        /// # 按 ID 检索单条记录(附带获取关联表的信息)
        ///
        /// 在 `id` 条件之上叠加检索参数中的附加条件，返回单条记录及其关联数据
        ///
        /// ## 参数
        /// * `id` - 要查询的记录 ID
        /// * `params` - 检索参数（字段条件按 Bean Searcher 规则，全局参数用原生 `_` 前缀）
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<ExVo>)` - 查询成功，记录不存在时 `extra` 为 `None`
        /// * `Err(SvcError)` - 查询失败
        #[db_unwrap]
        #[log_call]
        pub async fn search_ex_by_id<C>(
            id: U64,
            params: SearchParams,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<#ex_vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let one: Option<#ex_vo_name> = #dao_name::search_ex_by_id(id, &params, db)
                .await?
                .map(|m| m.into());
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(one))
        }
        });
    }

    // 生成search_ex_by_condition方法
    if !skip.contains("search_ex_by_condition") {
        let before_call = resolve_hook("search_ex_by_condition", false, &args.before, &args.before_write, Some(quote! { params }), None, true);
        let after_call = resolve_hook("search_ex_by_condition", false, &args.after, &args.after_write, Some(quote! { one }), Some(quote! { params }), true);
        generated_methods.push(quote! {
        /// # 按检索参数获取单条记录(附带获取关联表的信息)
        ///
        /// 根据检索参数查询记录及其关联数据
        ///
        /// ## 参数
        /// * `params` - 检索参数（字段条件按 Bean Searcher 规则，全局参数用原生 `_` 前缀）
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<ExVo>)` - 查询成功，记录不存在时 `extra` 为 `None`
        /// * `Err(SvcError)` - 查询失败
        #[db_unwrap]
        #[log_call]
        pub async fn search_ex_by_condition<C>(
            params: SearchParams,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<#ex_vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let one: Option<#ex_vo_name> = #dao_name::search_ex_by_condition(&params, db)
                .await?
                .map(|m| m.into());
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(one))
        }
        });
    }

    // 生成search_list_ex方法
    if !skip.contains("search_list_ex") {
        let before_call = resolve_hook("search_list_ex", false, &args.before, &args.before_write, Some(quote! { params }), None, true);
        let after_call = resolve_hook("search_list_ex", false, &args.after, &args.after_write, Some(quote! { all }), Some(quote! { params }), true);
        generated_methods.push(quote! {
        /// # 按检索参数查询记录列表(附带获取关联表的信息)
        ///
        /// 根据检索参数查询记录列表及其关联数据，支持排序
        ///
        /// ## 参数
        /// * `params` - 检索参数（字段条件按 Bean Searcher 规则，全局参数用原生 `_` 前缀）
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vec<ExVo>>)` - 查询成功，返回记录列表
        /// * `Err(SvcError)` - 查询失败
        #[db_unwrap]
        #[log_call]
        pub async fn search_list_ex<C>(
            params: SearchParams,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<Vec<#ex_vo_name>>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let all: Vec<#ex_vo_name> = #dao_name::search_list_ex(&params, db)
                .await?
                .into_iter()
                .map(|m| m.into())
                .collect();
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(Some(all)))
        }
        });
    }

    // 生成search_page_ex方法
    if !skip.contains("search_page_ex") {
        let before_call = resolve_hook("search_page_ex", false, &args.before, &args.before_write, Some(quote! { params }), None, true);
        let after_call = resolve_hook("search_page_ex", false, &args.after, &args.after_write, Some(quote! { list }), Some(quote! { params }), true);
        generated_methods.push(quote! {
        /// # 按检索参数分页查询记录列表(附带获取关联表的信息)
        ///
        /// 根据检索参数分页查询记录列表及其关联数据，支持排序
        ///
        /// ## 参数
        /// * `params` - 检索参数（含分页与排序；字段条件按 Bean Searcher 规则，全局参数用原生 `_` 前缀）
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<PageRx<ExVo>>)` - 查询成功，返回分页结果
        /// * `Err(SvcError)` - 查询失败
        #[db_unwrap]
        #[log_call]
        pub async fn search_page_ex<C>(
            params: SearchParams,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<PageRx<#ex_vo_name>>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            let (page_num, total, models) = #dao_name::search_page_ex(&params, db).await?;
            let list: Vec<#ex_vo_name> = models.into_iter().map(|m| m.into()).collect();
            #after_call
            Ok(Ro::success("查询成功".to_string()).extra(Some(PageRx::builder()
                .total(total)
                .page_num(page_num)
                .list(list)
                .build()
            )))
        }
        });
    }

    let expanded = quote! {
        // 生成的方法可按需 skip，这里逐个 allow，避免某个服务把写方法全 skip 后
        // 整片注入的 use 变成 unused_imports 警告
        #[allow(unused_imports)]
        use robotech::dao::{begin_transaction, build_like_condition, commit_transaction, SearchParams};
        #[allow(unused_imports)]
        use robotech::api::U64;
        #[allow(unused_imports)]
        use robotech::api::Ro;
        #[allow(unused_imports)]
        use robotech::api::rx::PageRx;
        #[allow(unused_imports)]
        use robotech::svc::SvcError;
        #[allow(unused_imports)]
        use robotech::macros::db_unwrap;
        #[allow(unused_imports)]
        use robotech::macros::log_call;
        #[allow(unused_imports)]
        use sea_orm::ConnectionTrait;
        #[allow(unused_imports)]
        use validator::Validate;

        use crate::dto::#dto_module::*;
        use crate::dao::#dao_name;
        use crate::mo::#module::ActiveModel;
        use crate::vo::{#vo_name, #ex_vo_name};

        #input

        impl #struct_name {
            #(#generated_methods)*
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}