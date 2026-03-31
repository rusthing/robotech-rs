use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ItemStruct;
use wheel_rs::str_utils::{split_camel_case, CamelFormat};

pub(crate) fn svc_macro(input: ItemStruct) -> TokenStream {
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
    let add_dto_name = format_ident!("{}AddDto", entity_name);
    let modify_dto_name = format_ident!("{}ModifyDto", entity_name);
    let save_dto_name = format_ident!("{}SaveDto", entity_name);
    let query_dto_name = format_ident!("{}QueryDto", entity_name);

    let mut generated_methods = Vec::new();

    // 生成add方法
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
            // 先校验dto
            add_dto.validate()?;

            let active_model: ActiveModel = add_dto.into();
            let one = #vo_name::from(#dao_name::insert(active_model, db).await?);
            Ok(Self::get_by_id(one.id as u64, Some(db))
                .await?
                .message("添加成功".to_string()))
        }
    });

    // 生成modify方法
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
            // 先校验dto
            modify_dto.validate()?;

            let id = modify_dto.id.unwrap();    // id经过校验，可以放心unwrap
            let active_model: ActiveModel = modify_dto.into();
            let one = #vo_name::from(#dao_name::update(active_model, db).await?);
            Ok(Self::get_by_id(one.id, Some(db))
                .await?
                .message("修改成功".to_string()))
        }
    });

    // 生成save方法
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
            if let Some(id) = save_dto.id {
                Self::modify(save_dto.into(), db).await
            } else {
                Self::add(save_dto.into(), db).await
            }
        }
    });

    // 生成del_by_id方法
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
            id: u64,
            #[skip_log]
            db: Option<&C>,
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            let one = Self::get_by_id(id, Some(db))
                .await?
                .extra
                .ok_or(SvcError::NotFound(id.to_string()))?;
            let rows_affected = #dao_name::delete(
                ActiveModel {
                    id: sea_orm::ActiveValue::Set(id as i64),
                    ..Default::default()
                },
                db,
            )
            .await?.rows_affected;
            if rows_affected == 0 {
                return Err(SvcError::NotFound(id.to_string()));
            }
            Ok(Ro::success("删除成功".to_string()).extra(Some(one)))
        }
    });

    // 生成get_by_id方法
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
            id: u64,
            #[skip_log]
            db: Option<&C>
        ) -> Result<Ro<#vo_name>, SvcError>
        where
            C: ConnectionTrait,
        {
            let one = #dao_name::get_by_id(id, db).await?.map(|v| #vo_name::from(v));
            Ok(Ro::success("查询成功".to_string()).extra(one))
        }
    });

    // 生成get_by_query_dto方法
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
            let mut condition = dto.to_condition();
            if let Some(keyword) = &dto._keyword {
                condition = condition.add(build_like_condition(keyword, #dao_name::LIKE_COLUMNS));
            }

            let one = #dao_name::get_by_condition(condition, db)
                .await?
                .map(|value| #vo_name::from(value));
            Ok(Ro::success("查询成功".to_string()).extra(one))
        }
    });

    let expanded = quote! {
        use robotech::dao::{begin_transaction, build_like_condition};
        use robotech::ro::Ro;
        use robotech::svc::SvcError;
        use robotech_macros::db_unwrap;
        use robotech_macros::log_call;
        use sea_orm::ConnectionTrait;
        use validator::Validate;

        use crate::dto::#dto_module::*;
        use crate::dao::#dao_name;
        use crate::model::#module::ActiveModel;
        use crate::vo::#vo_name;

        #input

        impl #struct_name {
            #(#generated_methods)*
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
