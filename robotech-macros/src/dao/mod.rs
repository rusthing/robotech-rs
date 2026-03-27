use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{bracketed, parenthesized, Expr, ItemStruct, Lit, LitStr, Token};
use wheel_rs::str_utils::{split_camel_case, CamelFormat};

/// 唯一键字段配置项
#[derive(Debug)]
struct UniqueKeyArgs {
    /// 唯一键的名称
    /// 如果是组合键，则用逗号分隔
    name: String,
    /// 唯一键的注释
    remark: String,
}

impl Parse for UniqueKeyArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // 解开圆括号
        let content;
        parenthesized!(content in input);

        let field_name: LitStr = content.parse()?;
        let _: Token![,] = content.parse()?;
        let field_remark: LitStr = content.parse()?;

        Ok(UniqueKeyArgs {
            name: field_name.value(),
            remark: field_remark.value(),
        })
    }
}

/// 外键配置项
#[derive(Debug)]
struct ForeignKeyArgs {
    /// 外键字段
    fk_column: String,
    /// 主键表
    pk_table: String,
    /// 主键表注释
    pk_table_remark: String,
}

impl Parse for ForeignKeyArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // 解开圆括号
        let content;
        parenthesized!(content in input);

        let fk_column: LitStr = content.parse()?;
        let _: Token![,] = content.parse()?;
        let pk_table: LitStr = content.parse()?;
        let _: Token![,] = content.parse()?;
        let pk_table_remark: LitStr = content.parse()?;

        Ok(ForeignKeyArgs {
            fk_column: fk_column.value(),
            pk_table: pk_table.value(),
            pk_table_remark: pk_table_remark.value(),
        })
    }
}

/// DAO方法生成宏参数解析
#[derive(Debug)]
pub(super) struct DaoArgs {
    /// 唯一键
    unique_keys: Vec<UniqueKeyArgs>,
    /// 外键
    foreign_keys: Vec<ForeignKeyArgs>,
    /// 模糊匹配字段
    like_columns: Vec<Expr>,
    /// 关联表
    related_tables: Vec<Expr>,
}

impl Parse for DaoArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut unique_keys = vec![];
        let mut foreign_keys = vec![];
        let mut like_columns = vec![];
        let mut related_tables = vec![];

        // 解析可选的参数列表
        while !input.is_empty() {
            // 解析标识符（参数名）
            let ident: Ident = input.parse()?;
            // 解析冒号
            let _colon: Token![:] = input.parse()?;

            if ident == "unique_keys" {
                let content;
                // 解开方括号
                bracketed!(content in input);
                let unique_keys_args = content.parse_terminated(UniqueKeyArgs::parse, Token![,])?;
                unique_keys = unique_keys_args.into_iter().collect();
            } else if ident == "foreign_keys" {
                let content;
                // 解开方括号
                bracketed!(content in input);
                let foreign_keys_args =
                    content.parse_terminated(ForeignKeyArgs::parse, Token![,])?;
                foreign_keys = foreign_keys_args.into_iter().collect();
            } else if ident == "like_columns" {
                let content;
                // 解开方括号
                bracketed!(content in input);
                // 解析逗号分隔的列表
                let parsed_args = content.parse_terminated(Expr::parse, Token![,])?;
                like_columns = parsed_args.into_iter().collect();
            } else if ident == "related_table" {
                let content;
                // 解开方括号
                bracketed!(content in input);
                // 解析逗号分隔的列表
                let parsed_args = content.parse_terminated(Expr::parse, Token![,])?;
                related_tables = parsed_args.into_iter().collect();
            } else {
                let error_msg = format!("未知的参数：{}", ident);
                return Err(syn::Error::new_spanned(&ident, error_msg));
            }

            // 如果还有更多参数，跳过逗号
            if !input.is_empty() {
                let _comma: Token![,] = input.parse()?;
            }
        }

        Ok(DaoArgs {
            unique_keys,
            foreign_keys,
            like_columns,
            related_tables,
        })
    }
}

pub(super) fn dao_macro(args: DaoArgs, input: ItemStruct) -> TokenStream {
    let DaoArgs {
        unique_keys,
        foreign_keys,
        like_columns,
        related_tables,
    } = args;

    let struct_name = &input.ident;
    // 提取文档注释
    let struct_remark = input
        .attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                if let syn::Meta::NameValue(meta) = &attr.meta {
                    if let Expr::Lit(expr_lit) = &meta.value {
                        if let Lit::Str(lit_str) = &expr_lit.lit {
                            return Some(lit_str.value());
                        }
                    }
                }
            }
            None
        })
        .collect::<Vec<String>>();
    // 解析结构体的名称，必须是Dao结尾，符合大驼峰命名规范
    let struct_name_str = struct_name.to_string();
    if !struct_name_str.ends_with("Dao") {
        return syn::Error::new_spanned(struct_name, "Struct name must end with 'Dao'")
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
    let table_name = module_name;

    // 生成 use 导入
    let generated_use_linkme = if !unique_keys.is_empty() || !foreign_keys.is_empty() {
        quote! {
            use linkme::distributed_slice;
        }
    } else {
        quote! {}
    };

    // 生成 UNIQUE_KEYS
    let generated_unique_keys = if unique_keys.is_empty() {
        quote! {}
    } else {
        let mut i = 0;
        let field_inits: Vec<TokenStream> = unique_keys
            .iter()
            .map(|field| {
                let fields_str = &field.name;
                let name_str = &field.remark;
                i += 1;
                let item_name = Ident::new(&format!("UNIQUE_FIELD_{}", i), Span::call_site());
                quote! {
                    #[distributed_slice(UNIQUE_KEYS_SLICE)]
                    static #item_name: (&str, &str, &str) = (#table_name, #fields_str, #name_str);
                }
            })
            .collect();

        quote! {
            use robotech::dao::UNIQUE_KEYS_SLICE;
            #(#field_inits)*
        }
    };

    // 生成 FOREIGN_KEYS
    let generated_foreign_keys = if foreign_keys.is_empty() {
        quote! {}
    } else {
        let fk_table = table_name;
        let table_remark = if struct_remark.is_empty() {
            // 检查是否有文档注释，没有则报错
            return syn::Error::new_spanned(
                &struct_name,
                "结构体必须添加文档注释，例如：/// 表描述信息",
            )
            .to_compile_error();
        } else {
            struct_remark[0].clone()
        };
        let fk_table_remark = table_remark;
        let mut i = 0;
        let field_inits: Vec<TokenStream> = foreign_keys
            .iter()
            .map(|key| {
                let fk_column = &key.fk_column;
                let pk_table = &key.pk_table;
                let pk_table_comment = &key.pk_table_remark;
                i += 1;
                let item_name = Ident::new(&format!("FOREIGN_KEY_{}", i), Span::call_site());
                quote! {
                    #[distributed_slice(FOREIGN_KEYS_SLICE)]
                    static #item_name: (&str, &str, &str, &str, &str) =
                        (#fk_table, #fk_table_remark, #fk_column, #pk_table, #pk_table_comment);
                }
            })
            .collect();

        quote! {
            use robotech::dao::FOREIGN_KEYS_SLICE;
            #(#field_inits)*
        }
    };

    // 生成成员变量
    let mut generated_members = Vec::new();

    // 生成 LIKE_COLUMNS
    if !like_columns.is_empty() {
        generated_members.push(quote! {
            /// # 模糊查询列
            pub const LIKE_COLUMNS: &[Column] = &[#(#like_columns),*];
        });
    }
    // 生成 RELATED_TABLES
    if !related_tables.is_empty() {
        generated_members.push(quote! {
            /// # 关联表
            pub const RELATED_TABLES: &[&str] = &[#(#related_tables),*];
        })
    }

    // 生成insert方法
    generated_members.push(quote! {
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
                .map_err(|e| DaoError::parse_db_err(e))
        }
    });

    // 生成update方法
    generated_members.push(quote! {
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
                .map_err(|e| DaoError::parse_db_err(e))
        }
    });

    // 生成delete方法
    generated_members.push(quote! {
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
                .map_err(|e| DaoError::parse_db_err(e))
        }
    });

    // 生成get_by_id方法
    generated_members.push(quote! {
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
                .map_err(|e| DaoError::parse_db_err(e))
        }
    });

    // 生成get方法
    generated_members.push(quote! {
        /// # 获取记录
        ///
        /// 根据提供的查询参数获取数据库中的记录
        ///
        /// ## 参数
        /// - `condition`: 查询条件
        /// - `db`: 数据库连接，如果未提供则使用全局数据库连接
        ///
        /// ## 返回值
        /// - `Result<Ro<Model>, SvcError>` - 查询结果封装为Ro对象，如果查询成功则返回封装了Model的Ro对象，否则返回错误信息
        pub async fn get<C>(condition: Condition, db: &C) -> Result<Option<Model>, DaoError>
        where
            C: ConnectionTrait,
        {
            Entity::find()
                .filter(condition)
                .one(db)
                .await
                .map_err(DaoError::from)
        }
    });

    // 生成also_related相关方法
    if !related_tables.is_empty() {
        // 从 related_tables 中提取表名
        let mut table_names = Vec::new();
        for expr in &related_tables {
            // 处理字符串字面量情况："oss_bucket"
            if let Expr::Lit(expr_lit) = expr {
                if let Lit::Str(lit_str) = &expr_lit.lit {
                    table_names.push(lit_str.value());
                }
            }
        }

        // 生成 Entity 引用
        let entity_refs: Vec<TokenStream> = table_names
            .iter()
            .map(|table_name| {
                let module_ident = Ident::new(table_name, Span::call_site());
                quote! { #module_ident::Entity }
            })
            .collect();

        // 生成 find_also_related 链式调用
        let find_also_related_calls = entity_refs.iter().map(|entity_ref| {
            quote! { .find_also_related(#entity_ref) }
        });

        // 使用实际表名作为模式匹配变量名（避免连字符等特殊字符）
        let pattern_vars: Vec<Ident> = table_names
            .iter()
            .map(|table_name| {
                // 将表名转换为合法的变量名（替换连字符等特殊字符）
                let var_name = table_name.replace('-', "_");
                Ident::new(&var_name, Span::call_site())
            })
            .collect();

        let result_tuple_elements: Vec<TokenStream> = std::iter::once(quote! { Model })
            .chain(pattern_vars.iter().map(|var| {
                quote! { #var::Model }
            }))
            .collect();

        let unwrap_calls: Vec<TokenStream> = pattern_vars
            .iter()
            .map(|var| {
                quote! { #var.unwrap() }
            })
            .collect();

        generated_members.push(quote! {
            /// # 根据 ID 查询记录 (附带获取关联表的信息)
            ///
            /// 此函数通过给定的 ID 查询单条记录，并同时获取关联的存储桶和对象信息
            ///
            /// ## 参数
            /// * `id` - 要查询的记录的唯一标识符
            /// * `db` - 数据库连接 trait 对象
            ///
            /// ## 返回值
            /// 返回一个包含主记录和关联记录的元组的 Option，如果查询失败则返回相应的错误信息
            /// 如果未找到匹配记录，则返回 None
            pub async fn get_by_id_also_related<C>(
                id: u64,
                db: &C,
            ) -> Result<Option<(#(#result_tuple_elements),*)>, DaoError>
            where
                C: ConnectionTrait,
            {
                Entity::find_by_id(id as i64)
                    #(#find_also_related_calls)*
                    .one(db)
                    .await
                    .map(|model_option| {
                        model_option.map(|(model, #(#pattern_vars),*)| {
                            (model, #(#unwrap_calls),*)
                        })
                    })
                    .map_err(|e| DaoError::parse_db_err(e))
            }
        })
    }

    let expanded = quote! {
        use robotech::dao::DaoError;
        use sea_orm::{
            ActiveModelTrait, ActiveValue, ConnectionTrait, EntityTrait, Condition, QueryFilter
        };

        use crate::model::#module::{ActiveModel, Column, Entity, Model};

        #generated_use_linkme

        #generated_unique_keys

        #generated_foreign_keys

        #input

        impl #struct_name {
            #(#generated_members)*
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
