use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Field, Fields, ItemStruct};
use wheel_rs::str_utils::{snake_to_pascal, split_camel_case, CamelFormat};

/// crud_dto宏：自动生成XxxAddDto、XxxModifyDto、XxxSaveDto
pub fn crud_dto_macro(input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;
    let struct_name_str = struct_name.to_string();

    // 验证结构体名称必须以Dto结尾
    if !struct_name_str.ends_with("Dto") {
        return syn::Error::new_spanned(struct_name, "Struct name must end with 'Dto'")
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
    let module_name = format_ident!("{}", struct_name_split.join("_").to_lowercase());

    let entity_name = struct_name_str.strip_suffix("Dto").unwrap();
    let add_dto_name = format_ident!("{}AddDto", entity_name);
    let modify_dto_name = format_ident!("{}ModifyDto", entity_name);
    let save_dto_name = format_ident!("{}SaveDto", entity_name);
    let query_dto_name = format_ident!("{}QueryDto", entity_name);

    let vis = &input.vis;
    let fields = &input.fields;

    // 处理字段
    let (add_fields, modify_fields, save_fields, query_fields, query_field_to_condition) =
        match fields {
            Fields::Named(named_fields) => {
                let mut add_field_tokens = Vec::new();
                let mut modify_field_tokens = Vec::new();
                let mut save_field_tokens = Vec::new();
                let mut query_field_tokens = Vec::new();
                let mut query_field_to_condition_tokens = Vec::new();

                for field in &named_fields.named {
                    if field.ident.as_ref().unwrap() == "id" {
                        continue;
                    }
                    add_field_tokens.push(process_field(field, "add"));
                    modify_field_tokens.push(process_field(field, "modify"));
                    save_field_tokens.push(process_field(field, "save"));
                    query_field_tokens.push(process_field(field, "query"));
                    query_field_to_condition_tokens
                        .push(add_query_field_to_condition_tokens(field));
                }
                (
                    quote! { #(#add_field_tokens)* },
                    quote! { #(#modify_field_tokens)* },
                    quote! { #(#save_field_tokens)* },
                    quote! { #(#query_field_tokens)* },
                    quote! { #(#query_field_to_condition_tokens)* },
                )
            }
            _ => (quote! {}, quote! {}, quote! {}, quote! {}, quote! {}),
        };

    let expanded = quote! {
        use derive_setters::Setters;
        use sea_orm::{ActiveValue, ColumnTrait, Condition};
        use typed_builder::TypedBuilder;
        use wheel_rs::serde::{option_option_serde, u64_option_serde};
        use crate::model::#module_name::{ActiveModel, Column};

        // AddDto
        #[derive(o2o::o2o, utoipa::ToSchema, Debug, Default, serde::Deserialize, validator::Validate, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[owned_into(ActiveModel)]
        #[ghosts(
            updator_id: Default::default(),
            create_timestamp: Default::default(),
            update_timestamp: Default::default(),
        )]
        #[builder]
        #vis struct #add_dto_name {
            #[into(match ~ {Some(v)=>ActiveValue::Set(v as i64),None=>ActiveValue::NotSet})]
            #[serde(with = "u64_option_serde")]
            #[builder(default, setter(strip_option))]
            pub id: Option<u64>,
            #add_fields
            #[serde(skip_deserializing)]
            #[into(creator_id, ActiveValue::Set(~ as i64))]
            pub _current_user_id: u64,
        }

        // ModifyDto
        #[derive(o2o::o2o, utoipa::ToSchema, Debug, Default, serde::Deserialize, validator::Validate, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[owned_into(ActiveModel)]
        #[ghosts(
            creator_id: Default::default(),
            create_timestamp: Default::default(),
            update_timestamp: Default::default(),
        )]
        #[builder]
        #vis struct #modify_dto_name {
            #[validate(required(message = "id不能为空"))]
            #[into(match ~ {Some(v)=>ActiveValue::Set(v as i64),None=>ActiveValue::NotSet})]
            #[builder(default, setter(strip_option))]
            #[serde(with = "u64_option_serde")]
            pub id: Option<u64>,
            #modify_fields
            #[serde(skip_deserializing)]
            #[into(updator_id, ActiveValue::Set(~ as i64))]
            pub _current_user_id: u64,
        }

        // SaveDto
        #[derive(o2o::o2o, utoipa::ToSchema, Debug, Default, serde::Deserialize, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[owned_into(#add_dto_name)]
        #[owned_into(#modify_dto_name)]
        #[builder]
        #vis struct #save_dto_name {
            #[serde(with = "u64_option_serde")]
            #[builder(default, setter(strip_option))]
            pub id: Option<u64>,
            #save_fields
            #[serde(skip_deserializing)]
            pub _current_user_id: u64,
        }

        // QueryDto
        #[derive(utoipa::ToSchema, utoipa::IntoParams, Debug, Default, serde::Deserialize, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[builder]
        #vis struct #query_dto_name {
            #[serde(with = "u64_option_serde")]
            #[builder(default, setter(strip_option))]
            pub id: Option<u64>,
            #query_fields
            #[builder(default, setter(strip_option))]
            pub _keyword: Option<String>,
            #[serde(skip_deserializing)]
            #[builder(default, setter(strip_option))]
            pub _current_user_id: Option<u64>,
        }

        impl #query_dto_name {
            pub fn to_condition(&self) -> Condition {
                let mut condition = Condition::all();

                // 处理 id 字段
                if let Some(id) = self.id {
                    condition = condition.add(Column::Id.eq(id as i64));
                }

                #query_field_to_condition

                condition
            }
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}

/// 处理单个字段，生成对应DTO的字段定义
fn process_field(field: &Field, target: &str) -> TokenStream {
    let field_name = &field.ident;
    let field_ty = &field.ty;
    let original_attrs = &field.attrs;

    // 检查是否已有关键属性
    let has_into = has_attr(original_attrs, "into");
    let has_validate = has_attr(original_attrs, "validate");
    let has_builder = has_attr(original_attrs, "builder");
    let has_serde = has_attr(original_attrs, "serde");

    // 生成属性
    let mut new_attrs = Vec::new();

    // 保留原有属性（除了validate，根据目标类型决定）
    for attr in original_attrs {
        if target == "modify" || target == "save" {
            // ModifyDto和SaveDto移除validate属性
            if !attr.path().is_ident("validate") {
                new_attrs.push(attr.clone());
            }
        } else {
            // AddDto保留所有属性
            new_attrs.push(attr.clone());
        }
    }

    // 如果是添加，且没有validate，自动生成检验属性
    if target == "add" && !has_validate {
        if let Some(attr) = generate_validate_attr(field) {
            new_attrs.push(attr);
        }
    }

    // 如果是添加和修改，且没有into，自动生成into属性
    if (target == "add" || target == "modify") && !has_into {
        new_attrs.push(generate_into_attr(field));
    }

    // 如果没有builder，自动生成builder属性
    if !has_builder {
        new_attrs.push(generate_builder_attr());
    }

    // 如果没有serde，自动生成serde属性
    if !has_serde {
        if let Some(attr) = generate_serde_attr(field) {
            new_attrs.push(attr);
        }
    }

    // 包装类型（添加Option）
    let wrapped_ty = wrap_type(field_ty);

    // 生成字段代码
    quote! {
        #(#new_attrs)*
        pub #field_name: #wrapped_ty,
    }
}

/// 提取字段注释（从文档注释或字段名）
fn extract_field_comment(field: &Field) -> String {
    // 尝试从文档注释提取
    for attr in &field.attrs {
        if let Some(comment) = get_doc_comment(attr) {
            return comment.trim().to_string();
        }
    }
    // 没有注释，使用字段名
    field.ident.as_ref().unwrap().to_string()
}

/// 获取文档注释
fn get_doc_comment(attr: &Attribute) -> Option<String> {
    // 简单解析文档注释：检查是否是 #\[doc = "...\] 格式
    if attr.path().is_ident("doc") {
        // 使用 syn 2.0 的方式：直接访问内部结构
        // 文档注释通常格式为 #[doc = "comment"]
        if let syn::Meta::NameValue(meta) = &attr.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(lit),
                ..
            }) = &meta.value
            {
                return Some(lit.value().trim().to_string());
            }
        }
    }
    None
}

/// 检查属性是否存在
fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

/// 生成validate属性
fn generate_validate_attr(field: &Field) -> Option<Attribute> {
    let ty = &field.ty;
    // 如果不是Option类型，添加validate宏
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = segment.ident.to_string();
            // 如果不是Option类型，添加validate属性
            if "Option" != type_name.as_str() {
                let field_required = format!("{}不能为空", extract_field_comment(field));
                return Some(if type_name.as_str() == "String" {
                    syn::parse_quote!(
                        #[validate(
                            required(message = #field_required),
                            length(min = 1, message = #field_required)
                        )]
                    )
                } else {
                    syn::parse_quote!(
                        #[validate(required(message = #field_required))]
                    )
                });
            }
        }
    }
    None
}

/// 生成into属性
fn generate_into_attr(field: &Field) -> Attribute {
    let ty = &field.ty;

    // 检查是否是Option类型
    if is_option_type(ty) {
        // Option<T>类型 -> 字段被包装成 Option<Option<T>>
        // into属性需要处理 Option<Option<T>> -> ActiveValue<Option<T>>
        // 注意：~ 代表原始值（Option<T>），但我们需要处理的是包装后的值（Option<Option<T>>）
        if let syn::Type::Path(type_path) = ty {
            if let Some(segment) = type_path.path.segments.last() {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        // 检查内部类型
                        if let syn::Type::Path(inner_path) = inner_ty {
                            if let Some(inner_segment) = inner_path.path.segments.last() {
                                let inner_type_name = inner_segment.ident.to_string();
                                return match inner_type_name.as_str() {
                                    "u64" | "u32" | "u16" | "u8" => {
                                        // Option<Option<u64>> -> ActiveValue<Option<i64>>
                                        // ~ 是 Option<u64>，需要转换为 Option<i64>
                                        syn::parse_quote!(
                                            #[into(match ~ {Some(v)=>ActiveValue::Set(v.map(|x| x as i64)),None=>ActiveValue::NotSet})]
                                        )
                                    }
                                    "i64" | "i32" | "i16" | "i8" => {
                                        // Option<Option<i64>> -> ActiveValue<Option<i64>>
                                        syn::parse_quote!(
                                            #[into(match ~ {Some(v)=>ActiveValue::Set(v),None=>ActiveValue::NotSet})]
                                        )
                                    }
                                    "String" => {
                                        // Option<Option<String>> -> ActiveValue<Option<String>>
                                        syn::parse_quote!(
                                            #[into(match ~ {Some(v)=>ActiveValue::Set(v),None=>ActiveValue::NotSet})]
                                        )
                                    }
                                    _ => {
                                        syn::parse_quote!(
                                            #[into(match ~ {Some(v)=>ActiveValue::Set(v),None=>ActiveValue::NotSet})]
                                        )
                                    }
                                };
                            }
                        }
                    }
                }
            }
        }
    } else {
        // 非Option类型 -> 字段被包装成 Option<T>
        // into属性需要处理 Option<T> -> ActiveValue<T>
        // 注意：~ 代表原始值（T），但我们需要处理的是包装后的值（Option<T>）
        // 所以我们需要 match ~ {Some(v)=>..., None=>...}
        if let syn::Type::Path(type_path) = ty {
            if let Some(segment) = type_path.path.segments.last() {
                let type_name = segment.ident.to_string();
                return match type_name.as_str() {
                    "u64" | "u32" | "u16" | "u8" => {
                        // Option<u64> -> ActiveValue<i64>
                        // ~ 是 Option<u64>，需要转换为 i64
                        syn::parse_quote!(
                            #[into(match ~ {Some(v)=>ActiveValue::Set(v as i64),None=>ActiveValue::NotSet})]
                        )
                    }
                    "i64" | "i32" | "i16" | "i8" => {
                        // Option<i64> -> ActiveValue<i64>
                        syn::parse_quote!(
                            #[into(match ~ {Some(v)=>ActiveValue::Set(v),None=>ActiveValue::NotSet})]
                        )
                    }
                    "String" => {
                        // String -> Option<String> -> ActiveValue<String>
                        // ~ 是 Option<String> 类型（包装后的值）
                        syn::parse_quote!(
                            #[into(match ~ {Some(v)=>ActiveValue::Set(v),None=>ActiveValue::NotSet})]
                        )
                    }
                    _ => {
                        syn::parse_quote!(
                            #[into(match ~ {Some(v)=>ActiveValue::Set(v),None=>ActiveValue::NotSet})]
                        )
                    }
                };
            }
        }
    }

    // 默认返回
    syn::parse_quote!(
        #[into(match ~ {Some(v)=>ActiveValue::Set(v),None=>ActiveValue::NotSet})]
    )
}

/// 生成builder属性
fn generate_builder_attr() -> Attribute {
    syn::parse_quote!(
        #[builder(default, setter(strip_option))]
    )
}

/// 生成serde属性
fn generate_serde_attr(field: &Field) -> Option<Attribute> {
    let ty = &field.ty;
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = segment.ident.to_string();
            match type_name.as_str() {
                "u64" => {
                    return Some(syn::parse_quote!(
                        #[serde(with = "u64_option_serde")]
                    ));
                }
                "Option" => {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                            if let syn::Type::Path(inner_path) = inner_ty {
                                if let Some(inner_segment) = inner_path.path.segments.last() {
                                    if inner_segment.ident == "String" {
                                        return Some(syn::parse_quote!(
                                            #[serde(deserialize_with = "option_option_serde::deserialize")]
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// 判断是否是Option类型
fn is_option_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

/// 包装类型（添加Option）
/// T -> Option<T>，Option<T> -> Option<Option<T>>
fn wrap_type(ty: &syn::Type) -> syn::Type {
    let type_str = quote! { #ty }.to_string();
    let wrapped_str = format!("Option<{}>", type_str);
    syn::parse_str(&wrapped_str).unwrap_or_else(|_| ty.clone())
}

fn add_query_field_to_condition_tokens(field: &Field) -> TokenStream {
    let field_name = field.ident.as_ref().unwrap();
    let column_name = format_ident!("{}", snake_to_pascal(&field_name.to_string()));
    // 判断字段类型
    let ty = &field.ty;
    if is_option_type(ty) {
        if let syn::Type::Path(type_path) = ty {
            if let Some(segment) = type_path.path.segments.last() {
                // Option<T> 类型 -> 在 QueryDto 中被包装为 Option<Option<T>>
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        if let syn::Type::Path(inner_path) = inner_ty {
                            if let Some(inner_segment) = inner_path.path.segments.last() {
                                let inner_type_name = inner_segment.ident.to_string();
                                let v = get_value_token_stream(&inner_type_name);
                                return quote! {
                                    if let Some(v) = self.#field_name.as_ref() {
                                        if let Some(v) = v {
                                            condition = condition.add(Column::#column_name.eq(#v));
                                        }else {
                                            condition = condition.add(Column::#column_name.is_null());
                                        }
                                    }
                                };
                            }
                        }
                    }
                }
            }
        }
    }

    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            // 非 Option 类型 -> 在 QueryDto 中被包装为 Option<T>
            let inner_type_name = segment.ident.to_string();
            let v = get_value_token_stream(&inner_type_name);
            return quote! {
                if let Some(v) = self.#field_name.as_ref() {
                    condition = condition.add(Column::#column_name.eq(#v));
                }
            };
        }
    }

    // 无法解析类型时报错
    syn::Error::new_spanned(
        ty,
        format!(
            "字段 '{}' 的类型无法识别，请使用基本类型或 Option<T> 包裹的基本类型",
            field_name
        ),
    )
    .to_compile_error()
    .into()
}

fn get_value_token_stream(type_name: &str) -> TokenStream {
    match type_name {
        "u8" => {
            quote! { *v as i8 }
        }
        "u16" => {
            quote! { *v as i16 }
        }
        "u32" => {
            quote! { *v as i32 }
        }
        "u64" => {
            quote! { *v as i64 }
        }
        "u128" => {
            quote! { *v as i128 }
        }
        "bool" => {
            quote! { *v }
        }
        _ => {
            quote! { v }
        }
    }
}
