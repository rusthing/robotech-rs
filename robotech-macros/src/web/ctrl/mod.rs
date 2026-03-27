use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ItemStruct;
use wheel_rs::str_utils::{split_camel_case, CamelFormat};

pub(crate) fn ctrl_macro(input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;

    // 解析结构体的名称，必须是Ctrl结尾，符合大驼峰命名规范
    let struct_name_str = struct_name.to_string();
    if !struct_name_str.ends_with("Ctrl") {
        return syn::Error::new_spanned(struct_name, "Struct name must end with 'Ctrl'")
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
    let entity_name = struct_name_split.join("");
    let module_name = struct_name_split.join("_").to_lowercase();
    let path = struct_name_split.join("/").to_lowercase();
    let module_path = format!("/{path}");
    let crud_path = module_path.clone();
    let save_path = format!("{module_path}/save");
    let del_by_id_path = format!("{crud_path}/{{id}}");
    let get_by_id_path = format!("{crud_path}/{{id}}");
    let get_by_query_dto = crud_path.clone();
    let dto_module = format_ident!("{module_name}_dto");
    let svc_name = format_ident!("{}Svc", entity_name);
    let vo_name = format_ident!("{}Vo", entity_name);
    let add_dto_name = format_ident!("{}AddDto", entity_name);
    let modify_dto_name = format_ident!("{}ModifyDto", entity_name);
    let save_dto_name = format_ident!("{}SaveDto", entity_name);
    let query_dto_name = format_ident!("{}QueryDto", entity_name);

    let mut generated_methods = Vec::new();

    // 生成add方法
    generated_methods.push(quote! {
        /// # 添加新的记录
        ///
        /// 该接口用于添加一个新的记录
        ///
        /// ## 请求体
        /// * `AddDto` - 包含记录信息的结构体
        ///
        /// ## 请求头
        /// * `USER_ID_HEADER_NAME` - 当前用户ID，必需项，类型为u64
        ///
        /// ## 返回值
        /// * 成功时返回添加后的信息的JSON格式数据
        /// * 失败时返回相应的错误信息
        ///
        /// ## 错误处理
        /// * 当缺少必要参数时，返回`ValidationError`错误
        /// * 当参数格式不正确时，返回`ValidationError`错误
        /// * 其他业务逻辑错误将按相应规则处理
        #[utoipa::path(
            post,
            path = #crud_path,
            responses((status = OK, body = Ro<#vo_name>))
        )]
        #[log_call]
        pub async fn add(
            headers: HeaderMap,
            Json(mut dto): Json<#add_dto_name>,
        ) -> Result<Json<Ro<#vo_name>>, CtrlError> {
            // 从header中解析当前用户ID，如果没有或解析失败则抛出ValidationError
            dto._current_user_id = get_current_user_id(&headers)?;

            let result = #svc_name::add::<DatabaseTransaction>(dto, None).await?;
            Ok(Json(result))
        }
    });

    // 生成modify方法
    generated_methods.push(quote! {
        /// # 修改记录的信息
        ///
        /// 该接口用于修改一个已存在记录的信息
        ///
        /// ## 请求体
        /// * `ModifyDto` - 包含待修改记录信息的结构体
        ///
        /// ## 请求头
        /// * `USER_ID_HEADER_NAME` - 当前用户ID，必需项，类型为u64
        ///
        /// ## 返回值
        /// * 成功时返回修改后的信息的JSON格式数据
        /// * 失败时返回相应的错误信息
        ///
        /// ## 错误处理
        /// * 当缺少必要参数时，返回`ValidationError`错误
        /// * 当参数格式不正确时，返回`ValidationError`错误
        /// * 其他业务逻辑错误将按相应规则处理
        #[utoipa::path(
            put,
            path = #crud_path,
            responses((status = OK, body = Ro<#vo_name>))
        )]
        #[log_call]
        pub async fn modify(
            headers: HeaderMap,
            Json(mut dto): Json<#modify_dto_name>,
        ) -> Result<Json<Ro<#vo_name>>, CtrlError> {
            // 从header中解析当前用户ID，如果没有或解析失败则抛出ValidationError
            dto._current_user_id = get_current_user_id(&headers)?;

            let result = #svc_name::modify::<DatabaseTransaction>(dto, None).await?;
            Ok(Json(result))
        }
    });

    // 生成save方法
    generated_methods.push(quote! {
        /// # 保存记录的信息
        ///
        /// 该接口用于保存记录的信息，如果记录不存在则创建新记录，如果记录已存在则更新记录
        ///
        /// ## 请求体
        /// * `SaveDto` - 包含记录信息的结构体
        ///
        /// ## 请求头
        /// * `USER_ID_HEADER_NAME` - 当前用户ID，必需项，类型为u64
        ///
        /// ## 返回值
        /// * 成功时返回保存后的信息的JSON格式数据
        /// * 失败时返回相应的错误信息
        ///
        /// ## 错误处理
        /// * 当缺少必要参数时，返回`ValidationError`错误
        /// * 当参数格式不正确时，返回`ValidationError`错误
        /// * 其他业务逻辑错误将按相应规则处理
        #[utoipa::path(
            post,
            path = #save_path,
            responses((status = OK, body = Ro<#vo_name>))
        )]
        #[log_call]
        pub async fn save(
            headers: HeaderMap,
            Json(mut dto): Json<#save_dto_name>,
        ) -> Result<Json<Ro<#vo_name>>, CtrlError> {
            // 从header中解析当前用户ID，如果没有或解析失败则抛出ValidationError
            dto._current_user_id = get_current_user_id(&headers)?;

            let result = #svc_name::save::<DatabaseTransaction>(dto, None).await?;
            Ok(Json(result))
        }
    });

    // 生成del_by_id方法
    generated_methods.push(quote! {
        /// # 删除记录
        ///
        /// 该接口用于删除一个已存在的记录
        ///
        /// ## 请求参数
        /// * `id` - 待删除记录的唯一标识符，类型为u64
        ///
        /// ## 错误处理
        /// * 当缺少参数`id`时，返回`ValidationError`错误
        /// * 当参数`id`格式不正确时，返回`ValidationError`错误
        /// * 当根据ID找不到对应记录时，返回相应的错误信息
        #[utoipa::path(
            delete,
            path = #del_by_id_path,
            params(
                ("id" = u64, Path, description = "记录的唯一标识符")
            ),
            responses((status = OK, body = Ro<#vo_name>))
        )]
        #[log_call]
        pub async fn del_by_id(
            Path(id): Path<u64>,
        ) -> Result<Json<Ro<#vo_name>>, CtrlError> {
            let ro = #svc_name::del_by_id::<DatabaseTransaction>(id, None).await?;
            Ok(Json(ro))
        }
    });

    // 生成get_by_id方法
    generated_methods.push(quote! {
        /// # 根据ID获取记录的信息
        ///
        /// 该接口通过查询参数中的ID获取对应记录的详细信息
        ///
        /// ## 查询参数
        /// * `id` - 记录的唯一标识符，类型为u64
        ///
        /// ## 返回值
        /// * 成功时返回对应的记录信息的JSON格式数据
        /// * 失败时返回相应的错误信息
        ///
        /// ## 错误处理
        /// * 当缺少参数`id`时，返回`ValidationError`错误
        /// * 当参数`id`格式不正确时，返回`ValidationError`错误
        /// * 当根据ID找不到对应记录时，返回相应的错误信息
        #[utoipa::path(
            get,
            path = #get_by_id_path,
            params(
                ("id" = u64, Path, description = "记录的唯一标识符")
            ),
            responses(
                (status = OK, body = Ro<#vo_name>)
            )
        )]
        #[log_call]
        pub async fn get_by_id(Path(id): Path<u64>) -> Result<Json<Ro<#vo_name>>, CtrlError> {
            let ro = #svc_name::get_by_id::<DatabaseConnection>(id, None).await?;
            Ok(Json(ro))
        }
    });

    // 生成get_by_query_dto方法
    generated_methods.push(quote! {
        /// # 根据查询参数获取记录的信息
        ///
        /// 该接口通过查询参数获取对应记录的详细信息
        ///
        /// ## 查询参数
        /// * `QueryDto` - 包含查询条件的结构体
        ///
        /// ## 返回值
        /// * 成功时返回对应的记录信息的JSON格式数据
        /// * 失败时返回相应的错误信息
        #[utoipa::path(
                    get,
                    path = #get_by_query_dto,
                    params(#query_dto_name),
                    responses(
                        (status = OK, body = Ro<#vo_name>)
                    )
        )]
        #[log_call]
        pub async fn get_by_query_dto(Query(dto): Query<#query_dto_name>) -> Result<Json<Ro<#vo_name>>, CtrlError> {
            let ro = #svc_name::get_by_query_dto::<DatabaseConnection>(dto, None).await?;
            Ok(Json(ro))
        }
    });

    let expanded = quote! {
        use axum::extract::{Path, Query};
        use axum::http::HeaderMap;
        use axum::response::Json;
        use robotech::macros::log_call;
        use robotech::ro::Ro;
        use robotech::web::ctrl_utils::get_current_user_id;
        use robotech::web::CtrlError;
        use sea_orm::{DatabaseConnection, DatabaseTransaction};
        use validator::Validate;

        use crate::dto::#dto_module::*;
        use crate::svc::#svc_name;
        use crate::vo::#vo_name;

        #(#generated_methods)*
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
