use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ItemFn, Pat, Token, parse::Parse, parse::ParseStream, parse_macro_input};

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
#[derive(Debug, Default)]
struct DaoArgs {
    insert: bool,
    update: bool,
    delete: bool,
    get_by_id: bool,
}

impl Parse for DaoArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut result = DaoArgs::default();
        let mut first = true;

        while !input.is_empty() {
            if !first {
                let _: Token![,] = input.parse()?;
            }
            first = false;

            let ident: Ident = input.parse()?;
            match ident.to_string().as_str() {
                "insert" => result.insert = true,
                "update" => result.update = true,
                "delete" => result.delete = true,
                "get_by_id" => result.get_by_id = true,
                "all" => {
                    result.insert = true;
                    result.update = true;
                    result.delete = true;
                    result.get_by_id = true;
                }
                _ => return Err(syn::Error::new_spanned(ident, "Unknown method name")),
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

    TokenStream::from(expanded)
}
