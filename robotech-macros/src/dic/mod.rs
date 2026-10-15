//! # 字典枚举（dic）属性宏
//!
//! 实现 `#[dic]` 属性宏：把 C 风格「字典枚举」的 **serde 表示统一成整数**（变体判别式），
//! 并同步把 OpenAPI schema 描述成整数。
//!
//! ## 为什么需要它
//!
//! Rust 的 `#[derive(Serialize, Deserialize)]` 对 C 风格枚举默认序列化为**变体名字符串**
//! （`JobType::FixedRate` → `"FixedRate"`），这与字典枚举的语义不符：
//!
//! - 枚举在数据库里是 `INT2` 列，注释也写着 `0=手动分派, 1=cron表达式, ...`，
//!   只有 HTTP 层是字符串，等于同一条数据在两层有两套表示，前端/调用方容易被误导；
//! - 字符串一旦改名（重构变体名）就是**破坏性变更**，而整数是稳定契约；
//! - 数字比字符串更省字节，也更容易在 `QueryDto` 的查询参数里使用（`?jobType=3`）。
//!
//! ## 生成的内容
//!
//! - `impl serde::Serialize`：输出变体判别式（`i16`）；
//! - `impl serde::Deserialize`：只接受整数，**非法值直接报错**（不静默回落到默认变体）；
//! - `impl utoipa::PartialSchema` + `impl utoipa::ToSchema`：schema 为 `integer`，
//!   description 里带上 `值=变体名` 的对照表，Swagger 上依然可读。
//!
//! ## 用法
//!
//! ```ignore
//! use robotech::macros::dic;
//!
//! #[dic]
//! #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr, Display, EnumString)]
//! #[strum(serialize_all = "kebab-case")]
//! pub enum JobType {
//!     /// 手动分派
//!     Manual = 0,
//!     /// cron 表达式
//!     Cron = 1,
//! }
//! ```
//!
//! 注意：**不要**再在 `#[derive(...)]` 里写 `Serialize` / `Deserialize` / `ToSchema`
//! —— 宏会自动把它们从派生列表里摘掉，避免与生成的实现冲突（写了也不报错，但没必要）。
//!
//! ## 约束
//!
//! - 只支持 C 风格枚举（所有变体都是单元变体，不带字段）；
//! - 判别式必须是整数字面量（可以省略，省略时按前一个变体 +1）；
//! - 判别式取值范围是 `i16`（与 `value()` / `from_i16` 的既有约定一致）。

use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{Expr, ExprLit, Fields, ItemEnum, Lit, Path, Token};

/// `#[dic]` 宏实现
pub fn dic_macro(input: ItemEnum) -> TokenStream {
    let enum_name = &input.ident;
    let enum_name_str = enum_name.to_string();

    if !input.generics.params.is_empty() {
        return syn::Error::new_spanned(&input.generics, "#[dic] 不支持泛型枚举")
            .to_compile_error();
    }

    // ---- 1. 只支持 C 风格枚举 ----
    for variant in &input.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return syn::Error::new_spanned(
                variant,
                "#[dic] 只支持 C 风格枚举：变体不能携带字段",
            )
            .to_compile_error();
        }
    }

    // ---- 2. 计算每个变体的判别式（缺省时按前一个 +1） ----
    let mut variant_paths = Vec::new();
    let mut literals = Vec::new();
    let mut mapping = Vec::new();
    let mut next: i64 = 0;

    for variant in &input.variants {
        let value = match &variant.discriminant {
            Some((_, expr)) => match int_value(expr) {
                Some(v) => {
                    next = v + 1;
                    v
                }
                None => {
                    return syn::Error::new_spanned(
                        expr,
                        "#[dic] 要求判别式是整数字面量",
                    )
                    .to_compile_error();
                }
            },
            None => {
                let v = next;
                next += 1;
                v
            }
        };

        if i16::try_from(value).is_err() {
            return syn::Error::new_spanned(
                variant,
                format!("#[dic] 判别式 {value} 超出 i16 范围"),
            )
            .to_compile_error();
        }

        let ident = &variant.ident;
        variant_paths.push(quote! { #enum_name::#ident });
        literals.push(Literal::i64_unsuffixed(value));
        mapping.push(format!("{value}={ident}"));
    }

    let description = format!("字典枚举（整数序列化）：{}", mapping.join(", "));

    // ---- 3. 摘掉与生成实现冲突的派生，再原样输出枚举本体 ----
    let attrs = strip_conflicting_derives(&input.attrs);
    let vis = &input.vis;
    let variants = &input.variants;

    quote! {
        #(#attrs)*
        #vis enum #enum_name {
            #variants
        }

        /// 字典枚举：序列化为整数（变体判别式），由 `#[dic]` 生成
        impl ::serde::Serialize for #enum_name {
            fn serialize<S>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                let value: i16 = match self {
                    #( #variant_paths => #literals, )*
                };
                serializer.serialize_i16(value)
            }
        }

        /// 字典枚举：从整数反序列化，非法值直接报错，由 `#[dic]` 生成
        impl<'de> ::serde::Deserialize<'de> for #enum_name {
            fn deserialize<D>(deserializer: D) -> ::core::result::Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                let value = <i16 as ::serde::Deserialize>::deserialize(deserializer)?;
                match value {
                    #( #literals => ::core::result::Result::Ok(#variant_paths), )*
                    _ => ::core::result::Result::Err(::serde::de::Error::custom(
                        ::std::format!("无效的 {} 值: {}", #enum_name_str, value),
                    )),
                }
            }
        }

        /// 字典枚举：OpenAPI schema 为整数，由 `#[dic]` 生成
        impl ::utoipa::PartialSchema for #enum_name {
            fn schema() -> ::utoipa::openapi::RefOr<::utoipa::openapi::schema::Schema> {
                ::utoipa::openapi::schema::ObjectBuilder::new()
                    .schema_type(::utoipa::openapi::schema::Type::Integer)
                    .description(Some(#description))
                    .into()
            }
        }

        impl ::utoipa::ToSchema for #enum_name {
            fn name() -> ::std::borrow::Cow<'static, str> {
                ::std::borrow::Cow::Borrowed(#enum_name_str)
            }
        }
    }
}

/// 从属性列表里摘掉 `Serialize` / `Deserialize` / `ToSchema` 三个派生，
/// 其余属性（含 doc 注释、`#[strum(...)]`、其他派生）原样保留。
fn strip_conflicting_derives(attrs: &[syn::Attribute]) -> Vec<TokenStream> {
    const CONFLICTING: &[&str] = &["Serialize", "Deserialize", "ToSchema"];

    let mut result = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("derive")
            && let Ok(paths) =
                attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)
        {
            let kept: Vec<&Path> = paths
                .iter()
                .filter(|path| {
                    let last = path
                        .segments
                        .last()
                        .map(|segment| segment.ident.to_string())
                        .unwrap_or_default();
                    !CONFLICTING.contains(&last.as_str())
                })
                .collect();

            if !kept.is_empty() {
                result.push(quote! { #[derive(#(#kept),*)] });
            }
            continue;
        }

        result.push(quote! { #attr });
    }
    result
}

/// 取整数字面量的值，兼容 `(3)` / 宏分组等写法
fn int_value(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(int), ..
        }) => int.base10_parse::<i64>().ok(),
        Expr::Group(group) => int_value(&group.expr),
        Expr::Paren(paren) => int_value(&paren.expr),
        _ => None,
    }
}
