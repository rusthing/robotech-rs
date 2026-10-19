//! # 字典枚举（dic）属性宏
//!
//! 实现 `#[dic]` 属性宏：把 C 风格「字典枚举」的**全部样板实现**一次性生成——
//! ① serde 表示统一成整数（变体判别式）；② OpenAPI schema 描述成整数；
//! ③ `Default` / `value()` / `from_i16()` / `From` 转换；④ sea-orm 的
//! `TryGetable` / `ValueType`（`feature = "server"` 下）。
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
//! 此外，每个字典枚举原本都要手写一大段**逐字重复**的样板代码（`Default`、`value()`、
//! `from_i16()`、`From` 双向、`TryGetable`、`ValueType`），既啰嗦又容易写错、写漏。
//!
//! ## 生成的内容
//!
//! **总是生成**：
//!
//! | 生成的实现 | 行为 |
//! |-----------|------|
//! | `serde::Serialize` | 输出变体判别式（`i16`） |
//! | `serde::Deserialize` | 只接受整数，**非法值直接报错**（不静默回落到默认变体） |
//! | `utoipa::PartialSchema` + `ToSchema` | schema 为 `integer`，description 带 `值=变体名` 对照表 |
//! | `X::value()` | `const fn`，返回 `i16` 判别式 |
//! | `X::from_i16(i16) -> Option<X>` | 越界返回 `None` |
//! | `From<X> for i16` | 无损，枚举 → 整数 |
//! | `From<X> for Option<i16>` | 无损，枚举 → `Some(整数)` |
//! | `sea_orm::TryGetable`（`feature = "server"`） | 只实现 `try_get_by`，**非法值报错** |
//! | `sea_orm::sea_query::ValueType`（`feature = "server"`） | `TinyInt`/`SmallInt` 均接受，**非法值报错** |
//!
//! **仅当写了 `#[dic(default = 变体名)]` 时额外生成**（这一组是「有损转换」，
//! 都需要一个兜底变体，所以绑在一起）：
//!
//! | 生成的实现 | 行为 |
//! |-----------|------|
//! | `Default` | 取 `#[dic(default = 变体名)]` 指定的变体 |
//! | `From<i16> for X` | 越界回落 `Default` |
//!
//! **故意不生成 `From<Option<i16>> for X`**（`None → Default`）。这正是「本来没值的
//! 变成有值」的入口：`#[vo]` 给可空列生成的转换是 `#[from(~.map(|v| v.into()))]`
//! （VO 字段写成 `Option<枚举>`），只用得到 `From<i16>`，`None` 原样保留；而 VO 字段
//! 写成非 `Option` 时是 `#[from(~.into())]，源头是 `Option<i16>` 时缺这个 impl 就
//! **直接编译不过**，逼作者把 VO 字段改成 `Option<枚举>`。
//! **宁可编译不过，也不要把数据库 NULL 静默解释成某个状态。**
//!
//! **不写 `default` 参数就不生成 `Default`** —— 因为「第一个变体就是默认值」往往只是
//! 变体书写顺序的巧合（例如 `ConnectorStatus` 的第一个变体是 `Running`，
//! 但业务默认值其实是 `Unassigned`）。默认值应当是**显式、可评审**的决定。
//!
//! 注意 `#[vo]` 宏对枚举字段生成的是 `#[from(~.into())]`（`i16` → 枚举），
//! 所以**凡是被 VO 使用的字典枚举都必须写 `default`**，否则编译不过。
//!
//! ## 可空列怎么配
//!
//! | 数据库列 | VO 字段写法 | NULL 的结果 |
//! |---------|-----------|------------|
//! | `int2 not null` | `X` | 不可能出现 NULL |
//! | `int2`（可空） | `Option<X>` | **保留为 `None`**（JSON 输出 `null`） |
//! | `int2`（可空） | `X` | **编译报错**（缺 `From<Option<i16>>`，故意留的缺口） |
//!
//! 也就是说：**列可空，VO 字段就必须写 `Option<枚举>`**，别让 NULL 被兜底变体吃掉。
//! 参照 `dtw_locator_bind.bind_target_type`（`Option<i16>` → VO 写 `Option<u16>`）。
//!
//! ## 用法
//!
//! ```ignore
//! use robotech::macros::dic;
//!
//! // 不需要 Default / From<i16>：不写 default 参数
//! #[dic]
//! #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr, Display, EnumString)]
//! #[strum(serialize_all = "kebab-case")]
//! pub enum JobType {
//!     /// 手动分派
//!     Manual = 0,
//!     /// cron 表达式
//!     Cron = 1,
//! }
//!
//! // 需要 Default / From<i16>：显式指定默认变体
//! #[dic(default = Unassigned)]
//! #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr, Display, EnumString)]
//! #[strum(serialize_all = "kebab-case")]
//! pub enum ConnectorStatus {
//!     Running = 0,
//!     // ...
//!     Unassigned = 3,
//!     // ...
//! }
//! ```
//!
//! **不要**再手写 `Default` / `value()` / `from_i16()` / `From` / `TryGetable` / `ValueType`，
//! 也不要在 `#[derive(...)]` 里写 `Serialize` / `Deserialize` / `ToSchema` / `Default`
//! —— 宏会自动把这些从派生列表里摘掉，避免与生成的实现冲突（写了不会报错，但没必要）。
//!
//! 枚举自带的方法（业务语义，例如 `AlertStatus::is_ended()`）仍可另开
//! `impl 枚举名 { ... }` 块手写，与宏生成的方法互不干扰。
//!
//! ## 约束
//!
//! - 只支持 C 风格枚举（所有变体都是单元变体，不带字段）；
//! - 判别式必须是整数字面量（可以省略，省略时按前一个变体 +1）；
//! - 判别式取值范围是 `i16`（与数据库 `INT2` 列一致）；
//! - 至少有一个变体。

use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, ExprLit, Fields, Ident, ItemEnum, Lit, Path, Token};

/// `#[dic]` 的参数：`#[dic]` 或 `#[dic(default = 变体名)]`
#[derive(Debug, Default)]
pub struct DicArgs {
    /// 显式指定的默认变体名
    default_variant: Option<Ident>,
}

impl Parse for DicArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(DicArgs::default());
        }

        let key: Ident = input.parse()?;
        if key != "default" {
            return Err(syn::Error::new(
                key.span(),
                "#[dic] 只支持一个可选参数：`default = 变体名`",
            ));
        }
        input.parse::<Token![=]>()?;
        let variant: Ident = input.parse()?;

        Ok(DicArgs {
            default_variant: Some(variant),
        })
    }
}

/// `#[dic]` 宏实现
pub fn dic_macro(args: DicArgs, input: ItemEnum) -> TokenStream {
    let enum_name = &input.ident;
    let enum_name_str = enum_name.to_string();

    if !input.generics.params.is_empty() {
        return syn::Error::new_spanned(&input.generics, "#[dic] 不支持泛型枚举")
            .to_compile_error();
    }

    // ---- 1. 只支持 C 风格枚举 ----
    for variant in &input.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return syn::Error::new_spanned(variant, "#[dic] 只支持 C 风格枚举：变体不能携带字段")
                .to_compile_error();
        }
    }

    if input.variants.is_empty() {
        return syn::Error::new_spanned(enum_name, "#[dic] 枚举至少要有一个变体").to_compile_error();
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
                    return syn::Error::new_spanned(expr, "#[dic] 要求判别式是整数字面量")
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

    // ---- 3. 解析默认变体（只在显式给了 default 参数时才生成 Default 及其衍生实现）----
    let default_variant = match &args.default_variant {
        Some(wanted) => {
            if !input.variants.iter().any(|v| v.ident == *wanted) {
                return syn::Error::new_spanned(
                    wanted,
                    format!("#[dic] default 指定的变体 `{wanted}` 不存在于枚举 {enum_name} 中"),
                )
                .to_compile_error();
            }
            Some(wanted.clone())
        }
        None => None,
    };

    // 这一组是「有损转换」，都需要一个兜底变体，所以绑在一起生成。
    // 不写 `default` 参数就不生成 —— 避免把「变体书写顺序」当成默认值的隐式约定。
    //
    // ⚠️ 故意**不**生成 `From<Option<i16>> for X`（`None → Default`）：
    // 那是把数据库 NULL 静默解释成某个状态的入口（「本来没值的变成有值」）。
    // `#[vo]` 对可空列生成的转换是 `~.map(|v| v.into())`（VO 字段写 `Option<枚举>`），
    // 只用 `From<i16>`、`None` 原样保留；而 VO 字段写非 `Option` 时源头是 `Option<i16>`
    // 就缺 impl 报编译错，逼作者改成 `Option<枚举>` —— 编译不过也比丢信息好。
    let default_impls = match &default_variant {
        Some(dv) => quote! {
            /// 字典枚举：默认值，由 `#[dic]` 生成
            impl ::core::default::Default for #enum_name {
                #[inline]
                fn default() -> Self {
                    Self::#dv
                }
            }

            impl ::core::convert::From<i16> for #enum_name {
                #[inline]
                fn from(v: i16) -> Self {
                    Self::from_i16(v).unwrap_or_default()
                }
            }
        },
        None => TokenStream::new(),
    };

    // ---- 4. 摘掉与生成实现冲突的派生，去掉变体上的 `#[default]`，再原样输出枚举本体 ----
    let attrs = strip_conflicting_derives(&input.attrs);
    let vis = &input.vis;

    let mut variants = input.variants.clone();
    for variant in variants.iter_mut() {
        variant.attrs.retain(|attr| !attr.path().is_ident("default"));
    }

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

        /// 字典枚举：整数与枚举互转，由 `#[dic]` 生成
        impl #enum_name {
            /// 取枚举的整数值（即数据库列里存的值）
            #[inline]
            pub const fn value(self) -> i16 {
                self as i16
            }

            /// 由整数构造枚举；越界返回 `None`
            pub fn from_i16(value: i16) -> ::core::option::Option<Self> {
                match value {
                    #( #literals => ::core::option::Option::Some(#variant_paths), )*
                    _ => ::core::option::Option::None,
                }
            }
        }

        impl ::core::convert::From<#enum_name> for i16 {
            #[inline]
            fn from(v: #enum_name) -> Self {
                v as i16
            }
        }

        impl ::core::convert::From<#enum_name> for ::core::option::Option<i16> {
            #[inline]
            fn from(v: #enum_name) -> Self {
                ::core::option::Option::Some(v as i16)
            }
        }

        #default_impls

        /// 字典枚举：sea-orm 读取，非法值报错，由 `#[dic]` 生成
        #[cfg(feature = "server")]
        impl ::sea_orm::TryGetable for #enum_name {
            fn try_get_by<I: ::sea_orm::ColIdx>(
                res: &::sea_orm::QueryResult,
                index: I,
            ) -> ::core::result::Result<Self, ::sea_orm::TryGetError> {
                let value = <i16 as ::sea_orm::TryGetable>::try_get_by(res, index)?;
                Self::from_i16(value).ok_or_else(|| {
                    ::sea_orm::TryGetError::DbErr(::sea_orm::DbErr::Custom(::std::format!(
                        "无效的 {} 值: {value}",
                        #enum_name_str
                    )))
                })
            }
        }

        /// 字典枚举：sea-orm 写入 / 作为列类型，非法值报错，由 `#[dic]` 生成
        #[cfg(feature = "server")]
        impl ::sea_orm::sea_query::ValueType for #enum_name {
            fn try_from(
                v: ::sea_orm::sea_query::Value,
            ) -> ::core::result::Result<Self, ::sea_orm::sea_query::ValueTypeErr> {
                match v {
                    ::sea_orm::sea_query::Value::TinyInt(Some(v)) => {
                        Self::from_i16(v as i16).ok_or(::sea_orm::sea_query::ValueTypeErr)
                    }
                    ::sea_orm::sea_query::Value::SmallInt(Some(v)) => {
                        Self::from_i16(v).ok_or(::sea_orm::sea_query::ValueTypeErr)
                    }
                    _ => ::core::result::Result::Err(::sea_orm::sea_query::ValueTypeErr),
                }
            }

            fn type_name() -> ::std::string::String {
                ::std::stringify!(#enum_name).to_owned()
            }

            fn array_type() -> ::sea_orm::sea_query::ArrayType {
                ::sea_orm::sea_query::ArrayType::SmallInt
            }

            fn column_type() -> ::sea_orm::sea_query::ColumnType {
                ::sea_orm::sea_query::ColumnType::SmallInteger
            }
        }
    }
}

/// 从属性列表里摘掉 `Serialize` / `Deserialize` / `ToSchema` / `Default` 四个派生，
/// 其余属性（含 doc 注释、`#[strum(...)]`、其他派生）原样保留。
fn strip_conflicting_derives(attrs: &[syn::Attribute]) -> Vec<TokenStream> {
    const CONFLICTING: &[&str] = &["Serialize", "Deserialize", "ToSchema", "Default"];

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
