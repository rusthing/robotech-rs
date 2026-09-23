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
/// 优先级：单方法钩子 > 写操作快捷方式（仅写方法生效）
fn resolve_hook(
    method_name: &str,
    is_write: bool,
    per_method: &HashMap<String, syn::Path>,
    fallback: &Option<syn::Path>,
) -> Option<TokenStream> {
    let fn_path = per_method
        .get(method_name)
        .or_else(|| if is_write { fallback.as_ref() } else { None });
    fn_path.map(|f| quote! { let _ = #f().await; })
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
        let before_call = resolve_hook("add", is_write, &args.before, &args.before_write);
        let after_call = resolve_hook("add", is_write, &args.after, &args.after_write);
        generated_methods.push(quote! {
        /// # 添加新记录
        ///
        /// 将提供的AddTo对象转换为ActiveModel并插入到数据库中
        ///
        /// ## 参数
        /// * `add_to` - 包含要添加记录信息的传输对象
        /// * `db` - 数据库连接或事务，如果未提供则创建连接及事务
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vo>)` - 添加成功，返回封装了新增Vo的Ro对象
        /// * `Err(SvcError)` - 添加失败，可能是因为违反唯一约束或其他数据库错误
        #[db_unwrap(transaction_required)]
        #[log_call]
        pub async fn add<C>(
            add_dto: #add_dto_name,
            #[skip_log]
            db: Option<&C>,
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            // 先校验dto
            add_dto.validate()?;

            let active_model: ActiveModel = add_dto.into();
            let one = #vo_name::from(#dao_name::insert(active_model, db).await?);
            #after_call
            Ok(Ro::success("添加成功".to_string()).extra(Some(one)))
        }
        });
    }

    // 生成modify方法
    if !skip.contains("modify") {
        let is_write = WRITE_METHODS.contains(&"modify");
        let before_call = resolve_hook("modify", is_write, &args.before, &args.before_write);
        let after_call = resolve_hook("modify", is_write, &args.after, &args.after_write);
        generated_methods.push(quote! {
        /// # 修改记录
        ///
        /// 根据提供的ModifyTo对象更新数据库中的相应记录
        ///
        /// ## 参数
        /// * `modify_to` - 包含要修改记录信息的传输对象，必须包含有效的ID
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vo>)` - 修改成功，返回封装了更新后Vo的Ro对象
        /// * `Err(SvcError)` - 修改失败，可能因为记录不存在、违反唯一约束或其他数据库错误
        #[db_unwrap(transaction_required)]
        #[log_call]
        pub async fn modify<C>(
            modify_dto: #modify_dto_name,
            #[skip_log]
            db: Option<&C>,
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            #before_call
            // 先校验dto
            modify_dto.validate()?;

            let active_model: ActiveModel = modify_dto.into();
            let one = #vo_name::from(#dao_name::update(active_model, db).await?);
            #after_call
            Ok(Ro::success("修改成功".to_string()).extra(Some(one)))
        }
        });
    }

    // 生成save方法
    if !skip.contains("save") {
        let before_call = resolve_hook("save", false, &args.before, &args.before_write);
        let after_call = resolve_hook("save", false, &args.after, &args.after_write);
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
                Self::modify(save_dto.into(), db).await
            } else {
                Self::add(save_dto.into(), db).await
            };
            #after_call
            result
        }
        });
    }

    // 生成del_by_id方法
    if !skip.contains("del_by_id") {
        let is_write = WRITE_METHODS.contains(&"del_by_id");
        let before_call = resolve_hook("del_by_id", is_write, &args.before, &args.before_write);
        let after_call = resolve_hook("del_by_id", is_write, &args.after, &args.after_write);
        generated_methods.push(quote! {
        /// # 删除记录
        ///
        /// 根据提供的ID删除数据库中的相应记录
        ///
        /// ## 参数
        /// * `id` - 要删除的记录的ID
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Ok(Ro<Vo>)` - 删除成功，返回封装了Vo的Ro对象
        /// * `Err(SvcError)` - 删除失败，可能因为记录不存在或其他数据库错误
        #[db_unwrap(transaction_required)]
        #[log_call]
        pub async fn del_by_id<C>(
            id: U64,
            #[skip_log]
            db: Option<&C>,
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
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
            Ok(Ro::success("删除成功".to_string()).extra(Some(one)))
        }
        });
    }

    // 生成del_by_query_dto方法
    if !skip.contains("del_by_query_dto") {
        let is_write = WRITE_METHODS.contains(&"del_by_query_dto");
        let before_call = resolve_hook("del_by_query_dto", is_write, &args.before, &args.before_write);
        let after_call = resolve_hook("del_by_query_dto", is_write, &args.after, &args.after_write);
        generated_methods.push(quote! {
        /// # 删除记录
        ///
        /// 根据提供的查询参数获取数据库中的记录
        ///
        /// ## 参数
        /// * `dto` - 查询参数
        /// * `db` - 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// * `Result<Ro<Vo>, SvcError>` - 查询结果封装为Ro对象，如果查询成功则返回封装了Vo的Ro对象，否则返回错误信息
        #[db_unwrap(transaction_required)]
        #[log_call]
        pub async fn del_by_query_dto<C>(
            dto: #query_dto_name,
            #[skip_log]
            db: Option<&C>,
        ) -> Result<Ro<()>, SvcError>
        where
            C: ConnectionTrait,
        {
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
            Ok(Ro::success(format!("删除了{}条记录", rows_affected).to_string()))
        }
        });
    }

    // 生成get_by_id方法
    if !skip.contains("get_by_id") {
        let before_call = resolve_hook("get_by_id", false, &args.before, &args.before_write);
        let after_call = resolve_hook("get_by_id", false, &args.after, &args.after_write);
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
        let before_call = resolve_hook("get_by_query_dto", false, &args.before, &args.before_write);
        let after_call = resolve_hook("get_by_query_dto", false, &args.after, &args.after_write);
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
        let before_call = resolve_hook("list_by_query_dto", false, &args.before, &args.before_write);
        let after_call = resolve_hook("list_by_query_dto", false, &args.after, &args.after_write);
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
        let before_call = resolve_hook("page_by_query_dto", false, &args.before, &args.before_write);
        let after_call = resolve_hook("page_by_query_dto", false, &args.after, &args.after_write);
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
        let before_call = resolve_hook("get_ex_by_id", false, &args.before, &args.before_write);
        let after_call = resolve_hook("get_ex_by_id", false, &args.after, &args.after_write);
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
        let before_call = resolve_hook("get_ex_by_query_dto", false, &args.before, &args.before_write);
        let after_call = resolve_hook("get_ex_by_query_dto", false, &args.after, &args.after_write);
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
        let before_call = resolve_hook("list_ex_by_query_dto", false, &args.before, &args.before_write);
        let after_call = resolve_hook("list_ex_by_query_dto", false, &args.after, &args.after_write);
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
        let before_call = resolve_hook("page_ex_by_query_dto", false, &args.before, &args.before_write);
        let after_call = resolve_hook("page_ex_by_query_dto", false, &args.after, &args.after_write);
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

    let expanded = quote! {
        use robotech::dao::{begin_transaction, build_like_condition};
        use robotech::api::U64;
        use robotech::api::Ro;
        use robotech::api::rx::PageRx;
        use robotech::svc::SvcError;
        use robotech::macros::db_unwrap;
        use robotech::macros::log_call;
        use sea_orm::ConnectionTrait;
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