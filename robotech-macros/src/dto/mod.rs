//! # DTO 属性宏
//!
//! 实现 `#[crud_dto]` 属性宏的参数解析与代码生成，为 DTO 结构体自动生成
//! AddDto、ModifyDto、SaveDto、QueryDto 四类衍生结构体（区分服务端与客户端模式）。

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream}, Attribute, Field, Fields, ItemStruct, LitStr,
    Token,
};
use wheel_rs::str_utils::{snake_to_pascal, split_camel_case, CamelFormat};

/// crud_dto宏参数
pub struct CrudDtoArgs {
    pub mo_crate: Option<String>,
}

impl Parse for CrudDtoArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut mo_crate = None;
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            let _: Token![=] = input.parse()?;
            if key == "mo_crate" {
                let value: LitStr = input.parse()?;
                mo_crate = Some(value.value());
            }
            if !input.is_empty() {
                let _: Token![,] = input.parse()?;
            }
        }
        Ok(CrudDtoArgs { mo_crate })
    }
}

/// crud_dto宏：自动生成XxxAddDto、XxxModifyDto、XxxSaveDto、XxxQueryDto
pub fn crud_dto_macro(args: CrudDtoArgs, input: ItemStruct) -> TokenStream {
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

    // 注释里的校验标签先统一体检：写错了直接编译报错，不要静默忽略
    if let Fields::Named(named_fields) = fields {
        for field in &named_fields.named {
            if let Err(error) = check_field_tags(field) {
                return error.to_compile_error().into();
            }
        }
    }

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

    // 处理字段 - 客户端模式（不含o2o/validator相关属性）
    let (client_add_fields, client_modify_fields, client_save_fields, client_query_fields) =
        match &input.fields {
            Fields::Named(named_fields) => {
                let mut add_field_tokens = Vec::new();
                let mut modify_field_tokens = Vec::new();
                let mut save_field_tokens = Vec::new();
                let mut query_field_tokens = Vec::new();

                for field in &named_fields.named {
                    if field.ident.as_ref().unwrap() == "id" {
                        continue;
                    }
                    add_field_tokens.push(process_field_client(field, "add"));
                    modify_field_tokens.push(process_field_client(field, "modify"));
                    save_field_tokens.push(process_field_client(field, "save"));
                    query_field_tokens.push(process_field_client(field, "query"));
                }
                (
                    quote! { #(#add_field_tokens)* },
                    quote! { #(#modify_field_tokens)* },
                    quote! { #(#save_field_tokens)* },
                    quote! { #(#query_field_tokens)* },
                )
            }
            _ => (quote! {}, quote! {}, quote! {}, quote! {}),
        };

    let mo_crate_path = args.mo_crate.as_deref().unwrap_or("crate");
    let mo_crate_token: TokenStream =
        syn::parse_str(mo_crate_path).unwrap_or_else(|_| quote! { crate });

    let expanded = quote! {
        use derive_setters::Setters;
        use typed_builder::TypedBuilder;
        use wheel_rs::serde::option_option_serde;

        use robotech::api::{U8, U16, U32, U64, U128};

        // ========== Server mode: full sea_orm/o2o code ==========
        #[cfg(feature = "server")]
        use std::fmt::{Display, Formatter};
        #[cfg(feature = "server")]
        use sea_orm::{ActiveValue, ColumnTrait, Condition};
        #[cfg(feature = "server")]
        use #mo_crate_token::mo::#module_name::{ActiveModel, Column};

        // AddDto
        #[cfg(feature = "server")]
        #[derive(o2o::o2o, utoipa::ToSchema, Debug, Clone, Default, serde::Serialize, serde::Deserialize, validator::Validate, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[owned_into(ActiveModel)]
        #[ghosts(
            updater_id: Default::default(),
            update_ms: Default::default(),
        )]
        #[builder]
        #vis struct #add_dto_name {
            #[into(match ~ {Some(v)=>ActiveValue::Set(v.into()),None=>ActiveValue::NotSet})]
            #[builder(default, setter(strip_option))]
            pub id: Option<U64>,
            #add_fields
            #[serde(skip_deserializing)]
            #[into(create_ms, match ~ {Some(v)=>ActiveValue::Set(v.into()),None=>ActiveValue::NotSet})]
            #[builder(default, setter(strip_option))]
            pub _current_ms: Option<U64>,
            #[serde(skip_deserializing)]
            #[into(creater_id, ActiveValue::Set(~.into()))]
            pub _current_user_id: U64,
        }

        // ModifyDto
        #[cfg(feature = "server")]
        #[derive(o2o::o2o, utoipa::ToSchema, Debug, Clone, Default, serde::Serialize, serde::Deserialize, validator::Validate, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[owned_into(ActiveModel)]
        #[ghosts(
            creater_id: Default::default(),
            create_ms: Default::default(),
        )]
        #[builder]
        #vis struct #modify_dto_name {
            #[validate(required(message = "id不能为空"))]
            #[into(match ~ {Some(v)=>ActiveValue::Set(v.into()),None=>ActiveValue::NotSet})]
            #[builder(default, setter(strip_option))]
            pub id: Option<U64>,
            #modify_fields
            #[serde(skip_deserializing)]
            #[into(update_ms, match ~ {Some(v)=>ActiveValue::Set(v.into()),None=>ActiveValue::NotSet})]
            #[builder(default, setter(strip_option))]
            pub _current_ms: Option<U64>,
            #[serde(skip_deserializing)]
            #[into(updater_id, ActiveValue::Set(~.into()))]
            pub _current_user_id: U64,
        }

        // SaveDto
        #[cfg(feature = "server")]
        #[derive(o2o::o2o, utoipa::ToSchema, Debug, Clone, Default, serde::Serialize, serde::Deserialize, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[owned_into(#add_dto_name)]
        #[owned_into(#modify_dto_name)]
        #[builder]
        #vis struct #save_dto_name {
            #[builder(default, setter(strip_option))]
            pub id: Option<U64>,
            #save_fields
            #[serde(skip_deserializing)]
            pub _current_user_id: U64,
            #[serde(skip_deserializing)]
            pub _current_ms: Option<U64>,
        }

        // QueryDto
        #[cfg(feature = "server")]
        #[derive(utoipa::ToSchema, utoipa::IntoParams, Debug, Clone, Default, serde::Serialize, serde::Deserialize, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[builder]
        #vis struct #query_dto_name {
            #[builder(default, setter(strip_option))]
            pub id: Option<U64>,
            #query_fields
            #[serde(rename = "_keyword")]
            #[builder(default, setter(strip_option))]
            pub _keyword: Option<String>,
            #[serde(skip_deserializing)]
            #[builder(default, setter(strip_option))]
            pub _current_user_id: Option<U64>,
            #[serde(rename = "_orderBy")]
            #[builder(default, setter(strip_option))]
            pub _order_by: Option<String>,
            #[serde(rename = "_page")]
            #[builder(default, setter(strip_option))]
            pub _page: Option<U64>,
            #[serde(rename = "_size")]
            #[builder(default, setter(strip_option))]
            pub _size: Option<U64>,
        }

        #[cfg(feature = "server")]
        impl Display for #query_dto_name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:?}", self)
            }
        }

        #[cfg(feature = "server")]
        impl #query_dto_name {
            pub fn to_condition(&self) -> Condition {
                let mut condition = Condition::all();

                if let Some(id) = self.id {
                    condition = condition.add(Column::Id.eq(id.value()));
                }

                #query_field_to_condition

                condition
            }
        }

        // ========== Client mode: pure data structures ==========
        #[cfg(not(feature = "server"))]
        #[derive(utoipa::ToSchema, Debug, Clone, Default, serde::Serialize, serde::Deserialize, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[builder]
        #vis struct #add_dto_name {
            #[builder(default, setter(strip_option))]
            pub id: Option<U64>,
            #client_add_fields
            #[serde(skip_serializing)]
            #[builder(default, setter(strip_option))]
            pub _current_ms: Option<U64>,
            #[serde(skip_serializing)]
            pub _current_user_id: U64,
        }

        #[cfg(not(feature = "server"))]
        #[derive(utoipa::ToSchema, Debug, Clone, Default, serde::Serialize, serde::Deserialize, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[builder]
        #vis struct #modify_dto_name {
            #[builder(default, setter(strip_option))]
            pub id: Option<U64>,
            #client_modify_fields
            #[serde(skip_serializing)]
            #[builder(default, setter(strip_option))]
            pub _current_ms: Option<U64>,
            #[serde(skip_serializing)]
            pub _current_user_id: U64,
        }

        #[cfg(not(feature = "server"))]
        #[derive(utoipa::ToSchema, Debug, Clone, Default, serde::Serialize, serde::Deserialize, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[builder]
        #vis struct #save_dto_name {
            #[builder(default, setter(strip_option))]
            pub id: Option<U64>,
            #client_save_fields
            #[serde(skip_serializing)]
            pub _current_user_id: U64,
            #[serde(skip_serializing)]
            pub _current_ms: Option<U64>,
        }

        #[cfg(not(feature = "server"))]
        #[derive(utoipa::ToSchema, Debug, Clone, Default, serde::Serialize, serde::Deserialize, Setters, TypedBuilder)]
        #[serde(default, rename_all = "camelCase")]
        #[builder]
        #vis struct #query_dto_name {
            #[builder(default, setter(strip_option))]
            pub id: Option<U64>,
            #client_query_fields
            #[serde(rename = "_keyword")]
            #[builder(default, setter(strip_option))]
            pub _keyword: Option<String>,
            #[serde(rename = "_orderBy")]
            #[builder(default, setter(strip_option))]
            pub _order_by: Option<String>,
            #[serde(rename = "_page")]
            #[builder(default, setter(strip_option))]
            pub _page: Option<U64>,
            #[serde(rename = "_size")]
            #[builder(default, setter(strip_option))]
            pub _size: Option<U64>,
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}

/// 处理单个字段，生成对应DTO的字段定义（客户端模式，不含o2o/validator相关属性）
fn process_field_client(field: &Field, _target: &str) -> TokenStream {
    let field_name = &field.ident;
    let field_ty = &field.ty;
    let original_attrs = &field.attrs;

    let has_builder = has_attr(original_attrs, "builder");
    let has_serde = has_attr(original_attrs, "serde");

    let mut new_attrs: Vec<Attribute> = Vec::new();

    // 保留原有属性（除了validate/into/owned_into/ghosts/db_default）
    for attr in original_attrs {
        let path = attr.path();
        if !path.is_ident("validate")
            && !path.is_ident("into")
            && !path.is_ident("owned_into")
            && !path.is_ident("ghosts")
            && !path.is_ident("db_default")
        {
            new_attrs.push(attr.clone());
        }
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

    // 映射无符号类型
    let mapped_ty = map_unsigned_type(field_ty);

    quote! {
        #(#new_attrs)*
        pub #field_name: Option<#mapped_ty>,
    }
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
    let has_db_default = has_attr(original_attrs, "db_default");

    // 生成属性
    let mut new_attrs = Vec::new();

    // 保留原有属性（除了validate和db_default，根据目标类型决定）
    for attr in original_attrs {
        if attr.path().is_ident("validate") {
            // ModifyDto和SaveDto移除validate属性
            if target == "add" {
                new_attrs.push(attr.clone());
            }
        } else if !attr.path().is_ident("db_default") {
            // db_default只是标记，不保留到生成的代码中
            new_attrs.push(attr.clone());
        }
    }

    // 如果是添加，且没有手动写validate，且没有db_default，自动生成检验属性
    if target == "add" && !has_validate && !has_db_default {
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

    // 映射无符号类型
    let mapped_ty = map_unsigned_type(field_ty);

    // 生成字段代码
    quote! {
        #(#new_attrs)*
        pub #field_name: Option<#mapped_ty>,
    }
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

/// 字段校验声明，来自文档注释里的 `@len` / `@range` 标签
///
/// 规范见 `robotech-macros/doc/dto-field-check.md`：约束只认标签，
/// 不从散文里猜（「最长 100 字符」这种描述对宏没有约束力）。
#[derive(Debug, Default)]
struct FieldCheck {
    /// 字符串长度区间，单位是字符数
    len: Option<Bound>,
    /// 是否写过 `@len`（`@len 0..` 会解析成「没有约束」，但那是显式声明的，
    /// 不能再回退到默认的 `min = 1`）
    len_declared: bool,
    /// 数值区间
    range: Option<Bound>,
}

/// 一个区间声明：`a..=b`、`a..`、`..=b`；空的一侧表示不限
#[derive(Debug)]
struct Bound {
    /// 端点原文，生成校验时按字段类型转字面量（整数 / 浮点）
    min_text: Option<String>,
    max_text: Option<String>,
}

/// 字段类型能做什么校验
enum FieldKind {
    /// `String`，必须有值
    Str,
    /// `Option<String>`，可以是 `None`
    OptStr,
    /// 数值类型；`float` 决定端点生成 `1` 还是 `1.0`
    Num { float: bool },
    /// 无符号包装类型（DTO 里的 `U8`/`U16`/`U32`/`U64`/`U128`），带包装类型名：
    /// 区间端点要写成 `U16::from(1)`，`range` 才能对上这些类型上的
    /// `ValidateRange<U16>` 实现
    WrappedUnsigned { wrapper: &'static str },
    /// 其它类型，只加必填校验
    Other,
}

/// 判断字段类型属于哪一类
fn field_kind(ty: &syn::Type) -> (FieldKind, bool) {
    let (ident, optional) = match ty {
        syn::Type::Path(type_path) => {
            let Some(segment) = type_path.path.segments.last() else {
                return (FieldKind::Other, false);
            };
            if segment.ident == "Option" {
                match extract_option_inner_type(type_path) {
                    Some(inner) => (inner, true),
                    None => return (FieldKind::Other, true),
                }
            } else {
                (segment.ident.to_string(), false)
            }
        }
        _ => return (FieldKind::Other, false),
    };

    let kind = match ident.as_str() {
        "String" => {
            if optional {
                FieldKind::OptStr
            } else {
                FieldKind::Str
            }
        }
        "i8" | "i16" | "i32" | "i64" => FieldKind::Num { float: false },
        "f32" | "f64" => FieldKind::Num { float: true },
        "u8" => FieldKind::WrappedUnsigned { wrapper: "U8" },
        "u16" => FieldKind::WrappedUnsigned { wrapper: "U16" },
        "u32" => FieldKind::WrappedUnsigned { wrapper: "U32" },
        "u64" => FieldKind::WrappedUnsigned { wrapper: "U64" },
        "u128" => FieldKind::WrappedUnsigned { wrapper: "U128" },
        _ => FieldKind::Other,
    };
    (kind, optional)
}

/// 解析字段注释里的校验标签，并检查标签和字段类型是否匹配
fn check_field_tags(field: &Field) -> syn::Result<FieldCheck> {
    let check = parse_check_tags(field)?;
    let field_name = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
    let (kind, _) = field_kind(&field.ty);

    if check.len.is_some() && !matches!(kind, FieldKind::Str | FieldKind::OptStr) {
        return Err(syn::Error::new_spanned(
            field,
            format!("字段 {field_name} 的 `@len` 只能用在 String / Option<String> 上"),
        ));
    }
    if check.range.is_some() {
        match kind {
            FieldKind::Num { .. } => {}
            FieldKind::WrappedUnsigned { .. } => {
                // 端点会生成带后缀的字面量（`1u16`），负数写不出来
                if let Some(bound) = &check.range {
                    let negative = [&bound.min_text, &bound.max_text]
                        .into_iter()
                        .flatten()
                        .any(|text| text.trim_start().starts_with('-'));
                    if negative {
                        return Err(syn::Error::new_spanned(
                            field,
                            format!("字段 {field_name} 是无符号类型，`@range` 的端点不能为负"),
                        ));
                    }
                }
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    field,
                    format!("字段 {field_name} 的 `@range` 只能用在数值字段上"),
                ));
            }
        }
    }
    Ok(check)
}

/// 解析字段注释里的 `@len` / `@range` 标签
fn parse_check_tags(field: &Field) -> syn::Result<FieldCheck> {
    let mut check = FieldCheck::default();
    let field_name = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
    for attr in &field.attrs {
        let Some(comment) = get_doc_comment(attr) else {
            continue;
        };
        let line = comment.trim();
        let Some(rest) = line.strip_prefix('@') else {
            continue;
        };
        let mut parts = rest.splitn(2, char::is_whitespace);
        let tag = parts.next().unwrap_or_default();
        let spec = parts.next().unwrap_or_default().trim();
        match tag {
            "len" | "range" => {
                let bound = parse_bound(spec).ok_or_else(|| {
                    syn::Error::new_spanned(
                        attr,
                        format!(
                            "字段 {field_name} 的 `@{tag}` 缺少合法区间，\
                             写法：`@{tag} 1..=100`、`@{tag} 1..`、`@{tag} ..=100`"
                        ),
                    )
                })?;
                let mut bound = bound;
                if tag == "len" {
                    check.len_declared = true;
                    // `@len 0..` / `@len 0..=N` 的 0 是「允许空串」的说法，
                    // 不生成最小长度；`0..` 等于没有任何长度约束
                    if bound.min_text.as_deref() == Some("0") {
                        bound.min_text = None;
                    }
                    if bound.min_text.is_none() && bound.max_text.is_none() {
                        continue;
                    }
                    check.len = Some(bound);
                } else {
                    check.range = Some(bound);
                }
            }
            other => {
                return Err(syn::Error::new_spanned(
                    attr,
                    format!("字段 {field_name} 用了未知的校验标签 `@{other}`，只支持 `@len` 与 `@range`"),
                ));
            }
        }
    }
    Ok(check)
}

/// `1..=100` / `1..` / `..=100` → 区间；两头都空视为不合法
fn parse_bound(spec: &str) -> Option<Bound> {
    let token = spec.split_whitespace().next()?;
    let (min, max) = if let Some((min, max)) = token.split_once("..=") {
        (min, max)
    } else if let Some(min) = token.strip_suffix("..") {
        (min, "")
    } else {
        return None;
    };

    let min_text = parse_number(min)?;
    let max_text = parse_number(max)?;
    if min_text.is_none() && max_text.is_none() {
        return None;
    }
    Some(Bound {
        min_text,
        max_text,
    })
}

/// 区间端点 → 字面量 token
///
/// 浮点字段必须给 `-180.0` 这种浮点字面量，否则 validator 拿 `f64` 和整数字面量
/// 比较会编译不过；整数（含字符数）保持 `180`。
fn number_token(raw: &str, float: bool) -> Option<TokenStream> {
    let text = if float && !raw.contains('.') {
        format!("{raw}.0")
    } else {
        raw.to_string()
    };
    text.parse().ok()
}

/// 区间端点：空串表示这一侧不限；只接受整数或小数（可带负号）
fn parse_number(raw: &str) -> Option<Option<String>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Some(None);
    }
    let digits_ok = raw.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '.');
    let has_digit = raw.chars().any(|c| c.is_ascii_digit());
    if digits_ok && has_digit {
        Some(Some(raw.to_string()))
    } else {
        None
    }
}

/// 字段的说法（取第一条不是校验标签的注释，没有就用字段名）
fn field_label(field: &Field) -> String {
    for attr in &field.attrs {
        if let Some(comment) = get_doc_comment(attr) {
            let line = comment.trim();
            if !line.is_empty() && !line.starts_with('@') {
                return line.to_string();
            }
        }
    }
    field.ident.as_ref().unwrap().to_string()
}

/// 生成validate属性
///
/// 口径（与 `doc/dto-field-check.md` 一致）：
/// - 字符串默认「不许空串」：`required` + `length(min = 1)`
/// - 写了 `@len 0..=N` 才算允许空串：去掉最小长度，只留上限
/// - 上限只认 `@len` 声明的值，没有就不限长（由数据库列类型兜底）
/// - 数值只在写了 `@range` 时才校验区间
fn generate_validate_attr(field: &Field) -> Option<Attribute> {
    let (kind, optional) = field_kind(&field.ty);
    let label = field_label(field);
    let check = parse_check_tags(field).unwrap_or_default();
    let mut metas: Vec<syn::Meta> = Vec::new();

    if !optional {
        let required = format!("{label}不能为空");
        metas.push(syn::parse_quote!(required(message = #required)));
    }

    match kind {
        FieldKind::Str | FieldKind::OptStr => match &check.len {
            Some(bound) => {
                let message = length_message(&label, bound);
                let min = bound.min_text.as_deref().and_then(|t| number_token(t, false));
                let max = bound.max_text.as_deref().and_then(|t| number_token(t, false));
                let meta = match (min, max) {
                    (Some(min), Some(max)) => {
                        syn::parse_quote!(length(min = #min, max = #max, message = #message))
                    }
                    (Some(min), None) => syn::parse_quote!(length(min = #min, message = #message)),
                    (None, Some(max)) => syn::parse_quote!(length(max = #max, message = #message)),
                    (None, None) => return None,
                };
                metas.push(meta);
            }
            None => {
                // 显式写了 `@len 0..`：允许空串、不限长，不加长度校验；
                // 没写才是默认口径：非 Option 的 String 不许空串，上限不设
                if !check.len_declared && !optional {
                    let empty = format!("{label}不能为空");
                    metas.push(syn::parse_quote!(length(min = 1, message = #empty)));
                }
            }
        },
        FieldKind::Num { float } => {
            if let Some(bound) = &check.range {
                let message = range_message(&label, bound);
                let min = bound.min_text.as_deref().and_then(|t| number_token(t, float));
                let max = bound.max_text.as_deref().and_then(|t| number_token(t, float));
                let meta = match (min, max) {
                    (Some(min), Some(max)) => {
                        syn::parse_quote!(range(min = #min, max = #max, message = #message))
                    }
                    (Some(min), None) => syn::parse_quote!(range(min = #min, message = #message)),
                    (None, Some(max)) => syn::parse_quote!(range(max = #max, message = #message)),
                    (None, None) => return None,
                };
                metas.push(meta);
            }
        }
        // 包装类型上的实现是 `ValidateRange<U16> for U16 / Option<U16>`，
        // 端点得构造成包装类型，所以生成 `U16::from(1u16)` 而不是裸 `1`：
        // 裸整数字面量会退化成 i32，落不到 `From<u16>` 上
        FieldKind::WrappedUnsigned { wrapper } => {
            if let Some(bound) = &check.range {
                let message = range_message(&label, bound);
                let suffix = wrapper.to_ascii_lowercase();
                let wrapper = syn::Ident::new(wrapper, proc_macro2::Span::call_site());
                let min = bound
                    .min_text
                    .as_deref()
                    .and_then(|t| number_token(&format!("{t}{suffix}"), false))
                    .map(|lit| quote!(#wrapper::from(#lit)));
                let max = bound
                    .max_text
                    .as_deref()
                    .and_then(|t| number_token(&format!("{t}{suffix}"), false))
                    .map(|lit| quote!(#wrapper::from(#lit)));
                let meta = match (min, max) {
                    (Some(min), Some(max)) => {
                        syn::parse_quote!(range(min = #min, max = #max, message = #message))
                    }
                    (Some(min), None) => syn::parse_quote!(range(min = #min, message = #message)),
                    (None, Some(max)) => syn::parse_quote!(range(max = #max, message = #message)),
                    (None, None) => return None,
                };
                metas.push(meta);
            }
        }
        FieldKind::Other => {}
    }

    if metas.is_empty() {
        return None;
    }
    Some(syn::parse_quote!(#[validate(#(#metas),*)]))
}

/// 长度校验的提示语
fn length_message(label: &str, bound: &Bound) -> String {
    match (&bound.min_text, &bound.max_text) {
        (Some(min), Some(max)) => format!("{label}长度需在 {min}~{max} 个字符之间"),
        (Some(min), None) => format!("{label}长度不能少于 {min} 个字符"),
        (None, Some(max)) => format!("{label}长度不能超过 {max} 个字符"),
        (None, None) => format!("{label}长度不合法"),
    }
}

/// 数值区间校验的提示语
fn range_message(label: &str, bound: &Bound) -> String {
    match (&bound.min_text, &bound.max_text) {
        (Some(min), Some(max)) => format!("{label}取值范围是 {min}~{max}"),
        (Some(min), None) => format!("{label}不能小于 {min}"),
        (None, Some(max)) => format!("{label}不能大于 {max}"),
        (None, None) => format!("{label}取值不合法"),
    }
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
                                    "u64" | "u32" | "u16" | "u8" | "U64" | "U32" | "U16" | "U8"
                                    | "U128" => {
                                        // Option<Option<U64>> -> ActiveValue<Option<i64>>
                                        // ~ 是 Option<U64>，需要转换为 Option<i64>
                                        syn::parse_quote!(
                                            #[into(match ~ {Some(v)=>ActiveValue::Set(v.map(|x| x.into())),None=>ActiveValue::NotSet})]
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
                                    "Duration" => {
                                        // Option<Option<Duration>> -> ActiveValue<Option<String>>
                                        syn::parse_quote!(
                                            #[into(match ~ {Some(v)=>ActiveValue::Set(v.map(|d| String::from(d))),None=>ActiveValue::NotSet})]
                                        )
                                    }
                                    _ => {
                                        syn::parse_quote!(
                                            #[into(match ~ {Some(v)=>ActiveValue::Set(v.map(|x| x.into())),None=>ActiveValue::NotSet})]
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
                    "u64" | "u32" | "u16" | "u8" | "U64" | "U32" | "U16" | "U8" | "U128" => {
                        // Option<U64> -> ActiveValue<i64>
                        // ~ 是 Option<U64>，需要转换为 i64
                        syn::parse_quote!(
                            #[into(match ~ {Some(v)=>ActiveValue::Set(v.into()),None=>ActiveValue::NotSet})]
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
                    "Duration" => {
                        // Duration -> Option<Duration> -> ActiveValue<String>
                        // ~ 是 Option<Duration> 类型（包装后的值）
                        syn::parse_quote!(
                            #[into(match ~ {Some(v)=>ActiveValue::Set(String::from(v)),None=>ActiveValue::NotSet})]
                        )
                    }
                    "f64" | "f32" => {
                        // Option<f64> -> ActiveValue<Decimal>
                        syn::parse_quote!(
                            #[into(match ~ {Some(v)=>ActiveValue::Set(sea_orm::prelude::Decimal::try_from(v).unwrap_or_default()),None=>ActiveValue::NotSet})]
                        )
                    }
                    _ => {
                        syn::parse_quote!(
                            #[into(match ~ {Some(v)=>ActiveValue::Set(v.into()),None=>ActiveValue::NotSet})]
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

/// # 生成serde属性
///
/// 仅对 `Option<String>` 字段生成（宏会把它包成 `Option<Option<String>>`）。
///
/// ## 为什么必须三个属性一起生成
///
/// 外层 `Option` 的语义是「DTO 里有没有这个字段」——这是部分更新（modify）的基础：
/// 没传的字段不更新，显式传 `null` 的才置空。`deserialize_with` 已经把这个语义
/// 落到了服务端（`null` → `Some(None)`），但如果不同时给**序列化侧**补
/// `serialize_with` + `skip_serializing_if`，客户端未设置的字段会被 serde 默认
/// 序列化成 JSON `null`，服务端再还原成 `Some(None)`，
/// 于是「没传」和「传了 null」被抹平成同一个值，所有未设置的 `Option<String>`
/// 字段都会被 UPDATE 成 NULL（典型症状：worker 上报执行结果时不设
/// `executor_instance`，INSERT 写入的实例号就被下一次 UPDATE 抹平）。
///
/// 三件套的效果：
///
/// | 字段值 | JSON | 服务端还原 | 落到列 |
/// |---|---|---|---|
/// | `None` | （key 不存在） | `None` | `ActiveValue::NotSet` 不更新 |
/// | `Some(None)` | `null` | `Some(None)` | `ActiveValue::Set(None)` 置空 |
/// | `Some(Some(v))` | `v` | `Some(Some(v))` | 设为 `v` |
///
/// 之所以只有 `Option<String>` 需要关心：`Option<u64>` / `Option<Duration>` 等
/// 类型没有被挂 `deserialize_with`，走 serde 默认行为（`null` → 外层 `None`），
/// 本来就是对的。
///
/// 字段上若已手写了 `#[serde(...)]`，本函数不会被调用（见调用处的 `has_serde` 判断），
/// 手写优先。
fn generate_serde_attr(field: &Field) -> Option<Attribute> {
    let ty = &field.ty;
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = segment.ident.to_string();
            match type_name.as_str() {
                "Option" => {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                            if let syn::Type::Path(inner_path) = inner_ty {
                                if let Some(inner_segment) = inner_path.path.segments.last() {
                                    if inner_segment.ident == "String" {
                                        return Some(syn::parse_quote!(
                                            #[serde(
                                                deserialize_with = "option_option_serde::deserialize",
                                                serialize_with = "option_option_serde::serialize",
                                                skip_serializing_if = "option_option_serde::is_none"
                                            )]
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
        "u8" | "u16" | "u32" | "u64" | "u128" | "U8" | "U16" | "U32" | "U64" | "U128" => {
            quote! { v.value() }
        }
        "bool" => {
            quote! { *v }
        }
        "String" | "i64" | "i32" | "i16" | "i8" | "f64" | "f32" => {
            quote! { v }
        }
        "Duration" => {
            quote! { String::from(v.clone()) }
        }
        _ => {
            quote! { v.value() }
        }
    }
}

fn map_unsigned_type(ty: &syn::Type) -> TokenStream {
    match ty {
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                let ident_str = segment.ident.to_string();

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

fn extract_option_inner_type(type_path: &syn::TypePath) -> Option<String> {
    if let syn::PathArguments::AngleBracketed(args) = &type_path.path.segments.last()?.arguments {
        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
            if let syn::Type::Path(inner_path) = inner_ty {
                return inner_path.path.segments.last().map(|s| s.ident.to_string());
            }
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    /// 解析一个只含单个字段的结构体，取出那个字段
    fn field_of(src: &str) -> Field {
        let item: ItemStruct = syn::parse_str(src).expect("结构体解析失败");
        match item.fields {
            Fields::Named(named) => named.named.into_iter().next().expect("没有字段"),
            _ => panic!("只支持命名字段"),
        }
    }

    /// 生成出来的 validate 属性（没有就是空串），去掉空格方便断言
    fn validate_of(field: &Field) -> String {
        generate_validate_attr(field)
            .map(|attr| attr.to_token_stream().to_string().replace(' ', ""))
            .unwrap_or_default()
    }

    fn field(name: &str, doc: &str, ty: &str) -> Field {
        field_of(&format!("struct Dto {{ /// {doc}\n pub {name}: {ty} }}"))
    }

    // ---- 默认口径：没写 @len 的字符串仍不许空串 ----

    #[test]
    fn untagged_string_keeps_min_one() {
        let attr = validate_of(&field("name", "名称", "String"));
        assert!(attr.contains("required"), "{attr}");
        assert!(attr.contains("length(min=1"), "{attr}");
        assert!(!attr.contains("max="), "没有声明上限就不该有 max: {attr}");
    }

    #[test]
    fn untagged_option_string_has_no_validation() {
        assert_eq!(validate_of(&field("remark", "备注", "Option<String>")), "");
    }

    // ---- @len ----

    #[test]
    fn len_zero_allows_empty_string() {
        let attr = validate_of(&field("target_id", "@len 0..=100", "String"));
        assert!(attr.contains("max=100"), "{attr}");
        assert!(!attr.contains("min="), "0 表示允许空串，不该有 min: {attr}");
    }

    #[test]
    fn len_one_forbids_empty_string() {
        let attr = validate_of(&field("code", "@len 1..=50", "String"));
        assert!(attr.contains("min=1"), "{attr}");
        assert!(attr.contains("max=50"), "{attr}");
    }

    #[test]
    fn len_without_upper_bound() {
        let attr = validate_of(&field("content", "@len 0..", "String"));
        // 0.. = 允许空串且不限长，等于没有长度约束，只剩必填
        assert!(!attr.contains("length"), "{attr}");
        assert!(attr.contains("required"), "{attr}");
    }

    #[test]
    fn len_applies_to_option_string() {
        let attr = validate_of(&field("remark", "@len 0..=50", "Option<String>"));
        assert!(attr.contains("max=50"), "{attr}");
        assert!(!attr.contains("required"), "Option 字段不强制必填: {attr}");
    }

    #[test]
    fn tag_must_start_the_line() {
        let attr = validate_of(&field("code", "编码 @len 1..=50", "String"));
        // 标签必须独占一行：跟散文混在一行时不当标签解析
        assert!(!attr.contains("max=50"), "{attr}");
    }

    // ---- @range ----

    #[test]
    fn range_on_float() {
        let attr = validate_of(&field("geo_longitude", "@range -180..=180", "f64"));
        assert!(attr.contains("range("), "{attr}");
        assert!(attr.contains("180"), "{attr}");
    }

    #[test]
    fn range_with_one_sided_bound() {
        let attr = validate_of(&field("count", "@range 0..", "i32"));
        assert!(attr.contains("range("), "{attr}");
        assert!(!attr.contains("max="), "{attr}");
    }

    // ---- 写错了要报出来 ----

    #[test]
    fn len_on_number_is_rejected() {
        let error = check_field_tags(&field("count", "@len 1..=10", "i32")).unwrap_err();
        assert!(error.to_string().contains("@len"), "{}", error);
    }

    #[test]
    fn range_on_string_is_rejected() {
        let error = check_field_tags(&field("code", "@range 1..=10", "String")).unwrap_err();
        assert!(error.to_string().contains("@range"), "{}", error);
    }

    #[test]
    fn range_on_unsigned_field_is_allowed() {
        // 无符号字段在 DTO 里是 U16，`range` 由 robotech 里 U16 的
        // `ValidateRange<U16>` 实现承接；端点要带类型后缀再包成包装类型
        assert!(check_field_tags(&field("level", "@range 1..=9", "u16")).is_ok());
        let attr = validate_of(&field("level", "@range 1..=9", "u16"));
        assert!(
            attr.contains("range(min=U16::from(1u16),max=U16::from(9u16)"),
            "{attr}"
        );
    }

    #[test]
    fn range_on_optional_unsigned_field_is_allowed() {
        let attr = validate_of(&field("bind_target_type", "@range 1..=2", "Option<u16>"));
        assert!(
            attr.contains("range(min=U16::from(1u16),max=U16::from(2u16)"),
            "{attr}"
        );
        assert!(!attr.contains("required"), "Option 字段不强制必填: {attr}");
    }

    #[test]
    fn negative_bound_on_unsigned_is_rejected() {
        let error = check_field_tags(&field("level", "@range -1..=9", "u16")).unwrap_err();
        assert!(error.to_string().contains("不能为负"), "{error}");
    }

    #[test]
    fn unknown_tag_is_rejected() {
        let error = check_field_tags(&field("code", "@lenn 1..=10", "String")).unwrap_err();
        assert!(error.to_string().contains("@lenn"), "{}", error);
    }

    #[test]
    fn malformed_bound_is_rejected() {
        let error = check_field_tags(&field("code", "@len 1-10", "String")).unwrap_err();
        assert!(error.to_string().contains("区间"), "{}", error);
    }
}
