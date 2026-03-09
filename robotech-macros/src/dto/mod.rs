use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Fields, ItemStruct};
use wheel_rs::str_utils::{split_camel_case, CamelFormat};

pub fn add_dto_macro(input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;
    let visibility = &input.vis;
    let fields = &input.fields;

    let suffix = "AddDto";
    // 解析结构体的名称，后缀必须是指定字符串，且符合大驼峰命名规范
    let struct_name_str = struct_name.to_string();
    if !struct_name_str.ends_with(suffix) {
        return syn::Error::new_spanned(
            struct_name,
            format!("Struct name must end with '{suffix}'"),
        )
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

    let fields = match fields {
        Fields::Named(named_fields) => {
            let field_list = &named_fields.named;
            quote! { #field_list }
        }
        _ => quote! {}, // 其他情况返回空
    };

    let expanded = quote! {
        #[derive(o2o::o2o, utoipa::ToSchema, Debug, serde::Deserialize, validator::Validate, Setters, TypedBuilder)]
        #[serde(rename_all = "camelCase")]
        #[owned_into(ActiveModel)]
        #[ghosts(
            updator_id: Default::default(),
            create_timestamp: Default::default(),
            update_timestamp: Default::default(),
        )]
        #[builder]
        #visibility struct #struct_name {
            #[into(match ~ {Some(v)=>ActiveValue::Set(v as i64),None=>ActiveValue::NotSet})]
            #[serde(with = "u64_option_serde")]
            #[builder(default, setter(strip_option))]
            pub id: Option<u64>,
            #fields
            #[serde(skip_deserializing)]
            #[into(creator_id, ActiveValue::Set(~ as i64))]
            pub current_user_id: u64,
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
pub fn modify_dto_macro(input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;
    let visibility = &input.vis;
    let fields = &input.fields;

    let suffix = "ModifyDto";
    // 解析结构体的名称，后缀必须是指定字符串，且符合大驼峰命名规范
    let struct_name_str = struct_name.to_string();
    if !struct_name_str.ends_with(suffix) {
        return syn::Error::new_spanned(
            struct_name,
            format!("Struct name must end with '{suffix}'"),
        )
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

    let fields = match fields {
        Fields::Named(named_fields) => {
            let field_list = &named_fields.named;
            quote! { #field_list }
        }
        _ => quote! {}, // 其他情况返回空
    };

    let expanded = quote! {
        #[derive(o2o::o2o, utoipa::ToSchema, Debug, serde::Deserialize, validator::Validate, Setters, TypedBuilder)]
        #[serde(rename_all = "camelCase")]
        #[owned_into(ActiveModel)]
        #[ghosts(
            creator_id: Default::default(),
            create_timestamp: Default::default(),
            update_timestamp: Default::default(),
        )]
        #[builder]
        #visibility struct #struct_name {
            #[validate(required(message = "缺少必要参数<id>"))]
            #[into(match ~ {Some(v)=>ActiveValue::Set(v as i64),None=>ActiveValue::NotSet})]
            #[builder(default, setter(strip_option))]
            #[serde(with = "u64_option_serde")]
            pub id: Option<u64>,
            #fields
            #[serde(skip_deserializing)]
            #[into(updator_id, ActiveValue::Set(~ as i64))]
            pub current_user_id: u64,
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
pub fn save_dto_macro(input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;
    let visibility = &input.vis;
    let fields = &input.fields;

    let suffix = "SaveDto";
    // 解析结构体的名称，后缀必须是指定字符串，且符合大驼峰命名规范
    let struct_name_str = struct_name.to_string();
    if !struct_name_str.ends_with(suffix) {
        return syn::Error::new_spanned(
            struct_name,
            format!("Struct name must end with '{suffix}'"),
        )
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
    let entity_name = struct_name_str.strip_suffix(suffix).unwrap();
    let add_dao_name = format_ident!("{}AddDto", entity_name);
    let modify_dao_name = format_ident!("{}ModifyDto", entity_name);

    let fields = match fields {
        Fields::Named(named_fields) => {
            let field_list = &named_fields.named;
            quote! { #field_list }
        }
        _ => quote! {}, // 其他情况返回空
    };

    let expanded = quote! {
        #[derive(o2o::o2o, utoipa::ToSchema, Debug, serde::Deserialize, Setters, TypedBuilder)]
        #[serde(rename_all = "camelCase")]
        #[serde_as]
        #[owned_into(#add_dao_name)]
        #[owned_into(#modify_dao_name)]
        #[builder]
        #visibility struct #struct_name {
            #[serde_as(as = "Option<String>")]
            #[builder(default, setter(strip_option))]
            pub id: Option<u64>,
            #fields
            #[serde(skip_deserializing)]
            pub current_user_id: u64,
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
