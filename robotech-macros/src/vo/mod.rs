use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Field, Fields};
use wheel_rs::str_utils::{split_camel_case, CamelFormat};

/// 检查字段是否已经有某个属性
fn has_attribute(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

/// 分析字段类型，生成对应的属性宏
fn generate_field_attrs(field: &Field) -> TokenStream {
    let ty = &field.ty;

    // 检查是否已经有 serde_as、from 或 builder 属性
    let has_from = has_attribute(&field.attrs, "from");
    let has_builder = has_attribute(&field.attrs, "builder");

    let mut attrs = TokenStream::new();

    if !has_from {
        // 添加 o2o 的 from 属性
        if let Some(from_attr) = generate_from_attr(ty) {
            attrs.extend(from_attr);
        }
    }

    if !has_builder {
        // 添加 builder 属性（仅针对 Option<T> 类型）
        if let Some(builder_attr) = generate_builder_attr(field) {
            attrs.extend(builder_attr);
        }
    }

    attrs
}

/// 生成 from 属性
fn generate_from_attr(ty: &syn::Type) -> Option<TokenStream> {
    Some(match ty {
        syn::Type::Path(type_path) => {
            let path_str = type_path.path.segments.last().unwrap().ident.to_string();

            // 先检查是否是 Option<T> 类型
            if is_option_type(ty) {
                if let Some(inner_ty) = extract_option_inner_type(type_path) {
                    return Some(match inner_ty.as_str() {
                        "u8" | "u16" | "u32" | "u64" | "u128" => {
                            quote! { #[from(~.map(|v|v.into()))] }
                        }
                        _ => return None,
                    });
                }
            }

            // 再处理普通类型
            Some(match path_str.as_str() {
                "u8" | "u16" | "u32" | "u64" | "u128" => quote! { #[from(~.into())] },
                _ => return None,
            })?
        }
        _ => return None,
    })
}

/// 提取 Option 类型的内部类型
fn extract_option_inner_type(type_path: &syn::TypePath) -> Option<String> {
    if let syn::PathArguments::AngleBracketed(args) = &type_path.path.segments.last()?.arguments {
        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
            if let syn::Type::Path(inner_path) = inner_ty {
                return Some(inner_path.path.segments.last()?.ident.to_string());
            }
        }
    }
    None
}

/// 生成 builder 属性（仅针对 Option<T> 类型）
fn generate_builder_attr(field: &Field) -> Option<TokenStream> {
    let ty = &field.ty;

    // 检查是否是 Option<T> 类型
    if is_option_type(ty) {
        return Some(quote! {
            #[builder(default, setter(into))]
        });
    }

    None
}

/// 将无符号整型映射为对应的U*类型，非无符号整型则保持原样
/// 支持 Option<T> 类型，例如 Option<u64> -> Option<U64>
fn map_unsigned_type(ty: &syn::Type) -> TokenStream {
    match ty {
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                let ident_str = segment.ident.to_string();

                // 处理 Option<T> 类型
                if ident_str == "Option" {
                    if let Some(inner) = extract_option_inner_type(type_path) {
                        return match inner.as_str() {
                            "u8" => quote! { Option<U8> },
                            "u16" => quote! { Option<U16> },
                            "u32" => quote! { Option<U32> },
                            "u64" => quote! { Option<U64> },
                            "u128" => quote! { Option<U128> },
                            _ => quote! { #ty },
                        };
                    }
                    return quote! { #ty };
                }

                // 处理普通类型
                match ident_str.as_str() {
                    "u8" => return quote! { U8 },
                    "u16" => return quote! { U16 },
                    "u32" => return quote! { U32 },
                    "u64" => return quote! { U64 },
                    "u128" => return quote! { U128 },
                    _ => {}
                }
            }
        }
        _ => {}
    }
    quote! { #ty }
}

/// 检查类型是否是 Option<T>
fn is_option_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                segment.ident == "Option"
            } else {
                false
            }
        }
        _ => false,
    }
}

pub fn vo_macro(input: DeriveInput) -> TokenStream {
    let struct_name = &input.ident;
    let vis = &input.vis;
    let struct_name_str = struct_name.to_string();

    // 验证结构体名称必须以Vo结尾
    if !struct_name_str.ends_with("Vo") {
        return syn::Error::new_spanned(struct_name, "Struct name must end with 'Vo'")
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

    // 处理字段
    let fields = match input.data {
        Data::Struct(data_struct) => match data_struct.fields {
            Fields::Named(fields_named) => {
                let processed_fields: Vec<_> = fields_named
                    .named
                    .iter()
                    .map(|field| {
                        let field_name = &field.ident;
                        let field_ty = map_unsigned_type(&field.ty);
                        let attrs = generate_field_attrs(field);

                        // 保留原有的注释和其他属性（除了 from/builder/serde）
                        let original_attrs: Vec<_> = field
                            .attrs
                            .iter()
                            .filter(|attr| {
                                !attr.path().is_ident("from")
                                    && !attr.path().is_ident("builder")
                                    && !attr.path().is_ident("serde")
                            })
                            .collect();

                        quote! {
                            #(#original_attrs)*
                            #attrs
                            pub #field_name: #field_ty,
                        }
                    })
                    .collect();

                quote! {
                    { #(#processed_fields)* }
                }
            }
            Fields::Unnamed(_) | Fields::Unit => {
                return quote! {
                    compile_error!("VO macro only supports named fields");
                };
            }
        },
        _ => {
            return quote! {
                compile_error!("VO macro can only be used on structs");
            };
        }
    };

    // 生成完整的结构体定义，包含所有必要的属性和派生宏
    let expanded = quote! {
        use o2o::o2o;
        use serde::Serialize;
        use serde_with::{serde_as, skip_serializing_none};
        use utoipa::ToSchema;
        use derive_setters::Setters;
        use typed_builder::TypedBuilder;
        use sea_orm::DerivePartialModel;
        use wheel_rs::serde::{u64_serde, u64_option_serde};
        use robotech::dao::{U8, U16, U32, U64, U128};
        use crate::mo::#module_name::{Entity, Model, ModelEx};

        #[skip_serializing_none]            // 忽略空字段(好像必须放在#[derive(o2o, Serialize)]的上方才能起效)
        #[derive(o2o, ToSchema, DerivePartialModel, Debug, Serialize, Clone, Setters, TypedBuilder)]
        #[from_owned(Model)]
        #[from_owned(ModelEx)]
        #[serde(rename_all = "camelCase")]
        #[serde_as]
        #[builder]
        #[sea_orm(entity = "Entity")]
        #vis struct #struct_name #fields
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
