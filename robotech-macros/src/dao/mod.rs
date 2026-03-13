use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{ItemStruct, LitStr, Token};

/// 唯一字段配置项
#[derive(Debug)]
struct UniqueFieldConfig {
    fields: String,
    name: String,
}

impl Parse for UniqueFieldConfig {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fields_lit: LitStr = input.parse()?;
        let _: Token![,] = input.parse()?;
        let name_lit: LitStr = input.parse()?;

        Ok(UniqueFieldConfig {
            fields: fields_lit.value(),
            name: name_lit.value(),
        })
    }
}

/// 定义唯一字段的过程宏参数
#[derive(Debug)]
pub(super) struct DefineUniqueFieldsArgs {
    table: String,
    fields: Vec<UniqueFieldConfig>,
}

impl Parse for DefineUniqueFieldsArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut table = String::new();

        // 尝试解析表名
        if input.peek(LitStr) {
            let table_lit: LitStr = input.parse()?;
            table = table_lit.value();
        }

        let mut fields = Vec::new();

        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }

            // 尝试解析元组 (fields, name)
            let content;
            syn::parenthesized!(content in input);
            fields.push(content.parse()?);
        }

        Ok(DefineUniqueFieldsArgs { table, fields })
    }
}

/// # 定义唯一字段的 HashMap
///
/// 用于快速初始化唯一字段的 HashMap 静态变量
///
/// ## 使用示例
/// ```rust
/// define_unique_fields!(
/// "oss_bucket",
/// ("name", "桶名称"),
/// );
/// ```
pub fn define_unique_fields_macro(args: DefineUniqueFieldsArgs) -> TokenStream {
    let table = &args.table;
    let field_inits: Vec<TokenStream> = args.fields.iter().map(|field| {
        let fields_str = &field.fields;
        let name_str = &field.name;
        quote! {
            push_unique_field(&mut hash_map, #table.to_string(), #fields_str.to_string(), #name_str.to_string());
        }
    }).collect();

    let expanded = quote! {
        use robotech::dao::{push_unique_field, eo::UniqueField};
        static UNIQUE_FIELDS: LazyLock<HashMap<String, UniqueField>> = LazyLock::new(|| {
            let mut hash_map = HashMap::new();
            #(#field_inits)*
            hash_map
        });
    };

    TokenStream::from(expanded)
}

/// 外键配置项
#[derive(Debug)]
struct ForeignKeyConfig {
    fk_column: String,
    pk_table: String,
    pk_table_comment: String,
}

impl Parse for ForeignKeyConfig {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fk_column_lit: LitStr = input.parse()?;
        let _: Token![,] = input.parse()?;
        let pk_table_lit: LitStr = input.parse()?;
        let _: Token![,] = input.parse()?;
        let pk_table_comment_lit: LitStr = input.parse()?;

        Ok(ForeignKeyConfig {
            fk_column: fk_column_lit.value(),
            pk_table: pk_table_lit.value(),
            pk_table_comment: pk_table_comment_lit.value(),
        })
    }
}

/// 定义外键的过程宏参数
#[derive(Debug)]
pub(super) struct DefineForeignKeysArgs {
    fk_table: String,
    fk_table_comment: String,
    keys: Vec<ForeignKeyConfig>,
}

impl Parse for DefineForeignKeysArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut fk_table = String::new();
        let mut fk_table_comment = String::new();

        // 尝试解析表名和注释
        if input.peek(LitStr) {
            let fk_table_lit: LitStr = input.parse()?;
            fk_table = fk_table_lit.value();

            // 尝试解析逗号
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;

                // 尝试解析表注释
                if input.peek(LitStr) {
                    let fk_table_comment_lit: LitStr = input.parse()?;
                    fk_table_comment = fk_table_comment_lit.value();
                }
            }
        }

        let mut keys = Vec::new();

        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }

            // 尝试解析元组 (fk_column, pk_table, pk_table_comment)
            let content;
            syn::parenthesized!(content in input);
            keys.push(content.parse()?);
        }

        Ok(DefineForeignKeysArgs {
            fk_table,
            fk_table_comment,
            keys,
        })
    }
}

/// # 定义外键的 HashMap
///
/// 用于快速初始化外键的 HashMap 静态变量
///
/// ## 使用示例
/// ```rust
/// define_foreign_keys!(
/// "oss_obj_ref", "对象引用",
/// ("obj_id", "oss_obj", "对象"),
/// ("bucket_id", "oss_bucket", "桶"),
/// );
/// ```
pub fn define_foreign_keys_macro(args: DefineForeignKeysArgs) -> TokenStream {
    let fk_table = &args.fk_table;
    let fk_table_comment = &args.fk_table_comment;

    let key_inits: Vec<TokenStream> = args
        .keys
        .iter()
        .map(|key| {
            let fk_column = &key.fk_column;
            let pk_table = &key.pk_table;
            let pk_table_comment = &key.pk_table_comment;
            quote! {
                push_feign_key(
                    &mut hash_map,
                    #fk_table.to_string(),
                    #fk_table_comment.to_string(),
                    #fk_column.to_string(),
                    #pk_table.to_string(),
                    #pk_table_comment.to_string(),
                );
            }
        })
        .collect();

    let expanded = quote! {
        use robotech::dao::{push_feign_key, eo::ForeignKey};
        static FOREIGN_KEYS: LazyLock<HashMap<String, ForeignKey>> = LazyLock::new(|| {
            let mut hash_map = HashMap::new();
            #(#key_inits)*
            hash_map
        });
    };

    TokenStream::from(expanded)
}

/// DAO方法生成宏参数解析
#[derive(Debug)]
pub(super) struct DaoArgs {
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

pub(super) fn dao_macro(args: DaoArgs, input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;
    // let struct_vis = &input.vis;
    // let struct_generics = &input.generics;

    let mut generated_methods = Vec::new();

    // 生成insert方法
    if args.insert {
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
                    .map_err(|e| DaoError::parse_db_err(e, &UNIQUE_FIELDS, &FOREIGN_KEYS))
            }
        });
    }

    // 生成update方法
    if args.update {
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
                // 保护创建者信息不能被修改
                active_model.creator_id = ActiveValue::NotSet;
                active_model.create_timestamp = ActiveValue::NotSet;
                // 当修改时间未设置时，设置修改时间
                if active_model.update_timestamp == ActiveValue::NotSet {
                    let now = ActiveValue::set(wheel_rs::time_utils::get_current_timestamp()? as i64);
                    active_model.update_timestamp = now;
                }
                // 执行数据库更新操作
                active_model
                    .update(db)
                    .await
                    .map_err(|e| DaoError::parse_db_err(e, &UNIQUE_FIELDS, &FOREIGN_KEYS))
            }
        });
    }

    // 生成delete方法
    if args.delete {
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
                    .map_err(|e| DaoError::parse_db_err(e, &UNIQUE_FIELDS, &FOREIGN_KEYS))
            }
        });
    }

    // 生成get_by_id方法
    if args.get_by_id {
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
            pub async fn get_by_id<C>(id: u64, db: &C) -> Result<Option<Model>, DaoError>
            where
                C: ConnectionTrait,
            {
                Entity::find_by_id(id as i64)
                    .one(db)
                    .await
                    .map_err(|e| DaoError::parse_db_err(e, &UNIQUE_FIELDS, &FOREIGN_KEYS))
            }
        });
    }

    let expanded = quote! {
        use robotech::dao::DaoError;
        use sea_orm::{
            ActiveModelTrait, ActiveValue, ConnectionTrait, EntityTrait,
        };

        #input

        impl #struct_name {
            #(#generated_methods)*
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
