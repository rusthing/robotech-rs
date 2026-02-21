use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ItemFn, Pat, Token, parse::Parse, parse::ParseStream, parse_macro_input};
use wheel_rs::str_utils::{CamelFormat, split_camel_case};

/// 宏参数解析结构
struct LogCallArgs {
    level: Option<Ident>,
}

impl Parse for LogCallArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // 如果输入为空，返回 None
        if input.is_empty() {
            return Ok(LogCallArgs { level: None });
        }

        // 解析 level = xxx 的形式
        let _level_key: Ident = input.parse()?;
        let _: Token![=] = input.parse()?;
        let level: Ident = input.parse()?;

        Ok(LogCallArgs { level: Some(level) })
    }
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

    // 如果没有指定 level，默认使用 debug
    let log_level = args.level.unwrap_or_else(|| format_ident!("debug"));

    let input = parse_macro_input!(item as ItemFn);

    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let fn_block = &input.block;
    let fn_vis = &input.vis;
    let fn_sig = &input.sig;

    // 收集参数信息
    let mut param_formats = Vec::new();
    let mut param_values = Vec::new();

    for arg in &input.sig.inputs {
        match arg {
            FnArg::Typed(pat_type) => {
                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    let param_name = &pat_ident.ident;
                    let param_name_str = param_name.to_string();

                    param_formats.push(format!("  {} = {{:?}}", param_name_str));
                    param_values.push(quote! { #param_name });
                }
            }
            FnArg::Receiver(_) => {
                param_formats.push("  self = {:?}".to_string());
                param_values.push(quote! { self });
            }
        }
    }

    // 构建新的函数体
    let expanded = if param_formats.is_empty() {
        // 没有参数的情况
        quote! {
            #fn_vis #fn_sig {
                log::#log_level!("→ 进入方法: {}()", #fn_name_str);
                #fn_block
            }
        }
    } else {
        // 有参数的情况 - 构建完整的格式字符串
        let format_string = format!(
            "→ 进入方法: {}() 参数: \n{}",
            fn_name_str,
            param_formats.join("\n")
        );

        quote! {
            #fn_vis #fn_sig {
                log::#log_level!(#format_string, #(#param_values),*);
                #fn_block
            }
        }
    };

    TokenStream::from(expanded)
}

/// DAO方法生成宏参数解析
#[derive(Debug)]
struct DaoArgs {
    exclude: bool,
    insert: bool,
    update: bool,
    delete: bool,
    get_by_id: bool,
}

impl Default for DaoArgs {
    fn default() -> Self {
        DaoArgs {
            exclude: false,
            insert: true,
            update: true,
            delete: true,
            get_by_id: true,
        }
    }
}

impl Parse for DaoArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(DaoArgs::default());
        }
        let mut result = DaoArgs {
            exclude: false,
            insert: false,
            update: false,
            delete: false,
            get_by_id: false,
        };
        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            match ident.to_string().to_lowercase().as_str() {
                "exclude" => {
                    result = DaoArgs::default();
                    result.exclude = true;
                    let _: Token![:] = input.parse()?;
                    continue;
                }
                "insert" => result.insert = !result.exclude,
                "update" => result.update = !result.exclude,
                "delete" => result.delete = !result.exclude,
                "get_by_id" => result.get_by_id = !result.exclude,
                "all" => {
                    return Ok(DaoArgs::default());
                }
                _ => return Err(syn::Error::new_spanned(ident, "Unknown method name")),
            }
            if let Err(_) = input.parse::<Token![,]>() {
                return Ok(result);
            }
        }

        Ok(result)
    }
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
    let methods_args = parse_macro_input!(attr as DaoArgs);
    let input = parse_macro_input!(item as syn::ItemStruct);
    let struct_name = &input.ident;
    // let struct_vis = &input.vis;
    // let struct_generics = &input.generics;

    let mut generated_methods = Vec::new();

    // 生成insert方法
    if methods_args.insert {
        generated_methods.push(quote! {
            /// # 插入记录
            ///
            /// 此函数负责向数据库中插入一个新的记录。它会自动处理以下逻辑：
            /// - 如果记录 ID 未设置（默认值），则生成一个新的唯一 ID
            /// - 如果创建时间戳未设置，则设置当前时间为创建和更新时间
            /// - 将修改者 ID 设置为创建者 ID（因为是新建记录）
            ///
            /// ## 参数
            /// * `active_model` - 包含待插入数据的 ActiveModel 实例
            /// * `db` - 数据库连接 trait 对象
            ///
            /// ## 返回值
            /// 返回插入后的完整 Model 实例，如果插入失败则返回相应的错误信息
            pub async fn insert<C>(mut active_model: ActiveModel, db: &C) -> Result<Model, DaoError>
            where
                C: ConnectionTrait,
            {
                // 当id为默认值(0)时生成ID
                if active_model.id == ActiveValue::NotSet {
                    active_model.id = ActiveValue::set(idworker::get_id_worker()?.next_id()? as i64);
                }
                // 当创建时间未设置时，设置创建时间和修改时间
                if active_model.create_timestamp == ActiveValue::NotSet {
                    let now = ActiveValue::set(wheel_rs::time_utils::get_current_timestamp()? as i64);
                    active_model.create_timestamp = now.clone();
                    active_model.update_timestamp = now;
                }
                // 添加时修改者就是创建者
                active_model.updator_id = active_model.creator_id.clone();
                // 执行数据库插入操作
                active_model
                    .insert(db)
                    .await
                    .map_err(|e| DaoError::parse_db_err(e, &UNIQUE_FIELDS))
            }
        });
    }

    // 生成update方法
    if methods_args.update {
        generated_methods.push(quote! {
            /// # 更新记录
            ///
            /// 此函数负责更新数据库中的现有记录。它会自动处理以下逻辑：
            /// - 如果更新时间戳未设置，则设置当前时间为更新时间
            /// - 更新完成后，重新查询并返回更新后的完整记录
            ///
            /// ## 参数
            /// * `active_model` - 包含待更新数据的 ActiveModel 实例
            /// * `db` - 数据库连接 trait 对象
            ///
            /// ## 返回值
            /// 返回更新后的完整 Model 实例，如果更新失败则返回相应的错误信息
            pub async fn update<C>(mut active_model: ActiveModel, db: &C) -> Result<Model, DaoError>
            where
                C: ConnectionTrait,
            {
                // 当修改时间未设置时，设置修改时间
                if active_model.update_timestamp == ActiveValue::NotSet {
                    let now = ActiveValue::set(wheel_rs::time_utils::get_current_timestamp()? as i64);
                    active_model.update_timestamp = now;
                }
                // 执行数据库更新操作
                active_model
                    .update(db)
                    .await
                    .map_err(|e| DaoError::parse_db_err(e, &UNIQUE_FIELDS))
            }
        });
    }

    // 生成delete方法
    if methods_args.delete {
        generated_methods.push(quote! {
            /// # 删除记录
            ///
            /// 此函数负责根据关键字段删除相应的记录
            ///
            /// ## 参数
            /// * `active_model` - 包含待删除数据的 ActiveModel 实例
            /// * `db` - 数据库连接 trait 对象
            ///
            /// ## 返回值
            /// 如果删除成功则返回 Ok(())，如果删除失败则返回相应的错误信息
            pub async fn delete<C>(active_model: ActiveModel, db: &C) -> Result<sea_orm::DeleteResult, DaoError>
            where
                C: ConnectionTrait,
            {
                active_model
                    .delete(db)
                    .await
                    .map_err(|e| DaoError::parse_db_err(e, &UNIQUE_FIELDS))
            }
        });
    }

    // 生成get_by_id方法
    if methods_args.get_by_id {
        generated_methods.push(quote! {
            /// # 根据ID查询相应记录
            ///
            /// 此函数负责根据提供的ID从数据库中查询对应的记录
            ///
            /// ## 参数
            /// * `id` - 要查询的记录的ID
            /// * `db` - 数据库连接 trait 对象
            ///
            /// ## 返回值
            /// 查询成功，如果记录存在，返回查询到的完整 Model 实例，如果不存在返回None; 查询失败则返回相应的错误信息
            pub async fn get_by_id<C>(id: i64, db: &C) -> Result<Option<Model>, DaoError>
            where
                C: ConnectionTrait,
            {
                Entity::find_by_id(id)
                    .one(db)
                    .await
                    .map_err(|e| DaoError::parse_db_err(e, &UNIQUE_FIELDS))
            }
        });
    }

    let expanded = quote! {
        #input

        impl #struct_name {
            #(#generated_methods)*
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{}", expanded);

    TokenStream::from(expanded)
}

/// db_unwrap属性宏参数解析
#[derive(Debug, Default)]
struct DbUnwrapArgs {
    /// 需要事务
    transaction_required: bool,
}

impl Parse for DbUnwrapArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(DbUnwrapArgs::default());
        }

        let ident: Ident = input.parse()?;
        match ident.to_string().to_lowercase().as_str() {
            "transaction_required" => Ok(DbUnwrapArgs {
                transaction_required: true,
            }),
            unknown => Err(syn::Error::new_spanned(
                ident,
                format!("Unknown argument: {unknown}"),
            )),
        }
    }
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

    let fn_vis = &input.vis;
    let fn_sig = &input.sig;

    let transaction_required = args.transaction_required;

    // 分析函数签名，提取参数和返回类型
    let has_db_param = input.sig.inputs.iter().any(|arg| match arg {
        FnArg::Typed(pat_type) => {
            if let Pat::Ident(pat_ident) = &*pat_type.pat {
                pat_ident.ident == "db"
            } else {
                false
            }
        }
        _ => false,
    });

    // 如果没有db参数，报错
    if !has_db_param {
        return syn::Error::new_spanned(
            &fn_sig,
            "Service query method must have a 'db: Option<&C>' parameter",
        )
        .to_compile_error()
        .into();
    }

    // 提取用户编写的代码块
    let user_block = &input.block;

    // 生成包装后的方法
    let expanded = quote! {
        #fn_vis #fn_sig {
            if let Some(db) = db {
                #user_block
            } else {
                let db_conn = robotech::db_conn::get_db_conn()?;
                let db = db_conn.as_ref();
                if #transaction_required {
                    // 开启事务
                    let tx = begin_transaction(db).await?;
                    let db = &tx;
                }
                #user_block
            }
        }
    };

    TokenStream::from(expanded)
}

/// SVC方法生成宏参数解析
#[derive(Debug)]
struct SvcArgs {
    exclude: bool,
    add: bool,
    modify: bool,
    save: bool,
    del: bool,
    get_by_id: bool,
}

impl Default for SvcArgs {
    fn default() -> Self {
        SvcArgs {
            exclude: false,
            add: true,
            modify: true,
            save: true,
            del: true,
            get_by_id: true,
        }
    }
}

impl Parse for SvcArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(SvcArgs::default());
        }
        let mut result = SvcArgs {
            exclude: false,
            add: false,
            modify: false,
            save: false,
            del: false,
            get_by_id: false,
        };
        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            match ident.to_string().to_lowercase().as_str() {
                "exclude" => {
                    result = SvcArgs::default();
                    result.exclude = true;
                    let _: Token![:] = input.parse()?;
                    continue;
                }
                "add" => result.add = !result.exclude,
                "modify" => result.modify = !result.exclude,
                "save" => result.save = !result.exclude,
                "del" => result.del = !result.exclude,
                "get_by_id" => result.get_by_id = !result.exclude,
                "all" => {
                    return Ok(SvcArgs::default());
                }
                _ => return Err(syn::Error::new_spanned(ident, "Unknown method name")),
            }
            if let Err(_) = input.parse::<Token![,]>() {
                return Ok(result);
            }
        }

        Ok(result)
    }
}

#[proc_macro_attribute]
pub fn svc(attr: TokenStream, item: TokenStream) -> TokenStream {
    let methods_args = parse_macro_input!(attr as SvcArgs);
    let input = parse_macro_input!(item as syn::ItemStruct);
    let struct_name = &input.ident;

    // 解析结构体的名称，必须是Svc结尾，符合大驼峰命名规范
    let struct_name_str = struct_name.to_string();
    if !struct_name_str.ends_with("Svc") {
        return syn::Error::new_spanned(struct_name, "Service struct name must end with 'Svc'")
            .to_compile_error()
            .into();
    }
    let struct_name_split = split_camel_case(&struct_name_str, CamelFormat::Upper);
    if struct_name_split.is_err() {
        return syn::Error::new_spanned(
            struct_name,
            "Service struct name must be a valid upper camel case",
        )
        .to_compile_error()
        .into();
    }
    let mut struct_name_split = struct_name_split.unwrap();
    struct_name_split.pop();
    let entity_name = struct_name_split.join("");
    let dao_name = format_ident!("{}Dao", entity_name);
    let vo_name = format_ident!("{}Vo", entity_name);
    let add_dto_name = format_ident!("{}AddDto", entity_name);
    let modify_dto_name = format_ident!("{}ModifyDto", entity_name);
    let save_dto_name = format_ident!("{}SaveDto", entity_name);

    let mut generated_methods = Vec::new();

    // 生成add方法
    if methods_args.add {
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
            pub async fn add<C>(
                add_dto: #add_dto_name,
                db: Option<&C>,
            ) -> Result<Ro<#vo_name>, SvcError>
            where
                C: ConnectionTrait,
            {
                let active_model: ActiveModel = add_dto.into();
                let one = #dao_name::insert(active_model, db).await?;
                Ok(Self::get_by_id(one.id as u64, Some(db))
                    .await?
                    .msg("添加成功".to_string()))
            }
        });
    }

    // 生成modify方法
    if methods_args.modify {
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
            pub async fn modify<C>(
                modify_dto: #modify_dto_name,
                db: Option<&C>,
            ) -> Result<Ro<#vo_name>, SvcError>
            where
                C: ConnectionTrait,
            {
                let id = modify_dto.id.unwrap();
                let active_model: ActiveModel = modify_dto.into();
                #dao_name::update(active_model, db).await?;
                Ok(Self::get_by_id(id, Some(db))
                    .await?
                    .msg("修改成功".to_string()))
            }
        });
    }

    // 生成save方法
    if methods_args.save {
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
                if save_dto.id.clone().is_some() {
                    Self::modify(save_dto.into(), db).await
                } else {
                    Self::add(save_dto.into(), db).await
                }
            }
        });
    }

    // 生成del方法
    if methods_args.del {
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
            pub async fn del<C>(
                id: u64,
                current_user_id: u64,
                db: Option<&C>,
            ) -> Result<Ro<#vo_name>, SvcError>
            where
                C: ConnectionTrait,
            {
                let del_model = Self::get_by_id(id, Some(db))
                    .await?
                    .get_extra()
                    .ok_or(SvcError::NotFound(id.to_string()))?;
                warn!(
                    "ID为<{}>的用户将删除oss_bucket中的记录: {:?}",
                    current_user_id,
                    del_model.clone()
                );
                #dao_name::delete(
                    ActiveModel {
                        id: sea_orm::ActiveValue::Set(id as i64),
                        ..Default::default()
                    },
                    db,
                )
                .await?;
                Ok(Ro::success("删除成功".to_string()).extra(Some(del_model)))
            }
        });
    }

    // 生成get_by_id方法
    if methods_args.get_by_id {
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
            pub async fn get_by_id<C>(id: u64, db: Option<&C>) -> Result<Ro<#vo_name>, SvcError>
            where
                C: ConnectionTrait,
            {
                let one = #dao_name::get_by_id(id as i64, db).await?;
                Ok(Ro::success("查询成功".to_string()).extra(one.map(|value| #vo_name::from(value))))
            }
        });
    }

    let expanded = quote! {
        #input

        impl #struct_name {
            #(#generated_methods)*
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{}", expanded);

    TokenStream::from(expanded)
}
