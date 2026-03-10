use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{ItemStruct, Token};
use wheel_rs::str_utils::{split_camel_case, CamelFormat};

/// Ctrl方法生成宏参数解析
#[derive(Debug)]
pub(crate) struct CtrlArgs {
    exclude: bool,
    add: bool,
    modify: bool,
    save: bool,
    del: bool,
    get_by_id: bool,
}

impl Default for CtrlArgs {
    fn default() -> Self {
        CtrlArgs {
            exclude: false,
            add: true,
            modify: true,
            save: true,
            del: true,
            get_by_id: true,
        }
    }
}

impl Parse for CtrlArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(CtrlArgs::default());
        }
        let mut result = CtrlArgs {
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
                    result = CtrlArgs::default();
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
                    return Ok(CtrlArgs::default());
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

pub(crate) fn ctrl_macro(args: CtrlArgs, input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;

    // 解析结构体的名称，必须是Svc结尾，符合大驼峰命名规范
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
    let path = struct_name_split.join("/");
    let crud_path = format!("/{path}");
    let save_path = format!("/{path}/save");
    let get_by_id_path = format!("/{path}/{{id}}");
    let entity_name = struct_name_split.join("");
    let svc_name = format_ident!("{}Svc", entity_name);
    let vo_name = format_ident!("{}Vo", entity_name);
    let add_dto_name = format_ident!("{}AddDto", entity_name);
    let modify_dto_name = format_ident!("{}ModifyDto", entity_name);
    let save_dto_name = format_ident!("{}SaveDto", entity_name);

    let mut generated_methods = Vec::new();

    // 生成add方法
    if args.add {
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
                dto.validate()?;

                // 从header中解析当前用户ID，如果没有或解析失败则抛出ValidationError
                dto.current_user_id = get_current_user_id(&headers)?;

                let result = #svc_name::add::<DatabaseTransaction>(dto, None).await?;
                Ok(Json(result))
            }
        });
    }

    // 生成modify方法
    if args.modify {
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
                dto.validate()?;

                // 从header中解析当前用户ID，如果没有或解析失败则抛出ValidationError
                dto.current_user_id = get_current_user_id(&headers)?;

                let result = #svc_name::modify::<DatabaseTransaction>(dto, None).await?;
                Ok(Json(result))
            }
        });
    }

    // 生成save方法
    if args.save {
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
                dto.current_user_id = get_current_user_id(&headers)?;

                let result = #svc_name::save::<DatabaseTransaction>(dto, None).await?;
                Ok(Json(result))
            }
        });
    }

    // 生成del方法
    if args.del {
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
                path = #crud_path,
                params(
                    ("id" = u64, Path, description = "记录的唯一标识符")
                ),
                responses((status = OK, body = Ro<#vo_name>))
            )]
            #[log_call]
            pub async fn del(
                Path(id): Path<u64>,
            ) -> Result<Json<Ro<#vo_name>>, CtrlError> {
                let ro = #svc_name::del::<DatabaseTransaction>(id, None).await?;
                Ok(Json(ro))
            }
        });
    }

    // 生成get_by_id方法
    if args.get_by_id {
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
    }

    let expanded = quote! {
        use axum::extract::Path;
        use axum::http::HeaderMap;
        use axum::response::Json;
        use robotech::macros::log_call;
        use robotech::ro::Ro;
        use robotech::web::ctrl_utils::get_current_user_id;
        use robotech::web::CtrlError;
        use sea_orm::{DatabaseConnection, DatabaseTransaction};
        use validator::Validate;

        #(#generated_methods)*
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
