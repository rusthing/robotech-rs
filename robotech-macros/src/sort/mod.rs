//! # sortable 属性宏
//!
//! 用一个属性标注 `XxxSvc`，即可获得「排序值由服务端维护」的全部钩子：
//!
//! - 新增：同组末尾 `max + 1`（树形按父编码追加 `001`）；
//! - 修改：忽略客户端排序值；分组列变化时新组追加末尾、旧组后续前移；
//! - 删除：同组后续记录前移 `−1`；
//! - 批量删除：只对出现空洞或重复的组整组重算一次。
//!
//! 宏展开后会生成 4 个模块内钩子函数，并给结构体补上
//! `#[svc(before_add = ..., before_modify = ..., after_del_by_id = ..., after_del_by_query_dto = ...)]`，
//! 因此业务代码只需要一行标注：
//!
//! ```ignore
//! #[sortable(kind = "flat", table = "adm_role", group = "group_id")]
//! pub struct AdmRoleSvc;
//!
//! #[sortable(kind = "tree", table = "adm_menu", scope = "app_id", parent = "parent_code")]
//! pub struct AdmMenuSvc;
//! ```
//!
//! 约定（与 `#[crud_dto]` 生成的结构体对齐）：
//! - DTO 字段名与列名一致（`group = "group_id"` 对应 DTO 的 `group_id`）；
//! - 排序字段默认 `sort_order`，树编码默认 `tree_code`；
//! - 树形父编码字段默认 `parent_code`（`#[req_only]`，不进 QueryDto）；
//! - 主键默认 `id`，操作人/更新时间列默认 `updater_id` / `update_ms`。

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Ident, ItemStruct, Lit, LitInt, LitStr, Token,
};

/// `#[sortable(...)]` 的元信息
pub struct SortableArgs {
    /// `flat` / `tree`
    kind: String,
    /// 表名
    table: String,
    /// 平铺分组列（同时也是 DTO 字段名）
    group: Option<String>,
    /// 树形外层隔离列（同时也是 DTO 字段名）
    scope: Option<String>,
    /// 树形父编码字段名
    parent: Option<String>,
    /// 树编码列名
    code: Option<String>,
    /// 平铺排序列名
    sort: Option<String>,
    /// 主键列名
    id: Option<String>,
    /// 已定义的排序元信息常量路径（给了就不再生成 `SORT_SPEC`）
    spec: Option<String>,
    /// 平铺排序值上限
    max: Option<i32>,
    /// 树编码最大长度
    max_length: Option<usize>,
    /// 同父最大兄弟数
    max_siblings: Option<usize>,
}

impl Parse for SortableArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = SortableArgs {
            kind: "flat".to_string(),
            table: String::new(),
            group: None,
            scope: None,
            parent: None,
            code: None,
            sort: None,
            id: None,
            spec: None,
            max: None,
            max_length: None,
            max_siblings: None,
        };

        let pairs =
            Punctuated::<SortableArg, Token![,]>::parse_terminated(input)?;
        for pair in pairs {
            match pair.key.to_string().as_str() {
                "kind" => args.kind = pair.value_string()?,
                "table" => args.table = pair.value_string()?,
                "group" => args.group = Some(pair.value_string()?),
                "scope" => args.scope = Some(pair.value_string()?),
                "parent" => args.parent = Some(pair.value_string()?),
                "code" => args.code = Some(pair.value_string()?),
                "sort" => args.sort = Some(pair.value_string()?),
                "id" => args.id = Some(pair.value_string()?),
                "spec" => args.spec = Some(pair.value_string()?),
                "max" => args.max = Some(pair.value_int()? as i32),
                "max_length" => args.max_length = Some(pair.value_int()? as usize),
                "max_siblings" => {
                    args.max_siblings = Some(pair.value_int()? as usize)
                }
                unknown => {
                    return Err(syn::Error::new_spanned(
                        pair.key,
                        format!("Unknown argument: {unknown}"),
                    ));
                }
            }
        }

        if args.table.is_empty() && args.spec.is_none() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "sortable 必须指定 table 或 spec",
            ));
        }
        if !matches!(args.kind.as_str(), "flat" | "tree") {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "sortable 的 kind 只能是 \"flat\" 或 \"tree\"",
            ));
        }

        Ok(args)
    }
}

/// 一个 `key = value` 参数
struct SortableArg {
    key: Ident,
    value: Lit,
}

impl Parse for SortableArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        let _: Token![=] = input.parse()?;
        // 允许字符串与整数字面量；布尔值留给未来扩展
        let value: Lit = input.parse()?;
        Ok(Self { key, value })
    }
}

impl SortableArg {
    fn value_string(&self) -> syn::Result<String> {
        match &self.value {
            Lit::Str(value) => Ok(value.value()),
            other => Err(syn::Error::new_spanned(
                other,
                "该参数需要字符串字面量",
            )),
        }
    }

    fn value_int(&self) -> syn::Result<u64> {
        match &self.value {
            Lit::Int(value) => value.base10_parse(),
            other => Err(syn::Error::new_spanned(
                other,
                "该参数需要整数字面量",
            )),
        }
    }
}

/// 生成排序钩子与 `#[svc]` 属性
pub fn sortable_macro(args: SortableArgs, input: ItemStruct) -> TokenStream {
    let entity_name = input
        .ident
        .to_string()
        .strip_suffix("Svc")
        .unwrap_or(&input.ident.to_string())
        .to_string();
    let add_dto = format_ident!("{}AddDto", entity_name);
    let modify_dto = format_ident!("{}ModifyDto", entity_name);
    let query_dto = format_ident!("{}QueryDto", entity_name);
    let vo = format_ident!("{}Vo", entity_name);

    let sort_field = format_ident!("{}", args.sort.as_deref().unwrap_or("sort_order"));
    let code_field = format_ident!("{}", args.code.as_deref().unwrap_or("tree_code"));
    let parent_field = format_ident!("{}", args.parent.as_deref().unwrap_or("parent_code"));
    let group_field = args.group.as_ref().map(|name| format_ident!("{name}"));
    let scope_field = args.scope.as_ref().map(|name| format_ident!("{name}"));
    let group_col = args
        .group
        .as_ref()
        .map(|name| LitStr::new(name, proc_macro2::Span::call_site()));
    let scope_col = args
        .scope
        .as_ref()
        .map(|name| LitStr::new(name, proc_macro2::Span::call_site()));

    // 指定了 `spec` 就复用已有常量，不再在模块内重复定义；否则按元信息生成。
    let (spec_def, spec_path) = match &args.spec {
        Some(path) => {
            let path: syn::Path = match syn::parse_str(path) {
                Ok(path) => path,
                Err(err) => {
                    return syn::Error::new(
                        proc_macro2::Span::call_site(),
                        format!("sortable 的 spec 不是合法路径：{err}"),
                    )
                    .to_compile_error()
                }
            };
            (quote! {}, quote! { #path })
        }
        None => {
            let table = LitStr::new(&args.table, proc_macro2::Span::call_site());
            let generated = match args.kind.as_str() {
                "flat" => {
                    let mut spec = quote! {
                        ::robotech::dao::sort::FlatSort::new(#table)
                    };
                    if let Some(col) = &group_col {
                        spec = quote! { #spec.with_group(#col) };
                    }
                    if let Some(max) = args.max {
                        let max = LitInt::new(&max.to_string(), proc_macro2::Span::call_site());
                        spec = quote! { #spec.with_max(#max) };
                    }
                    quote! {
                        /// 排序元信息：表名、分组列与上限
                        const SORT_SPEC: ::robotech::dao::sort::FlatSort = #spec;
                    }
                }
                _ => {
                    let mut spec = quote! {
                        ::robotech::dao::sort::TreeSort::new(#table)
                    };
                    if let Some(col) = &scope_col {
                        spec = quote! { #spec.with_scope(#col) };
                    }
                    if args.max_length.is_some() || args.max_siblings.is_some() {
                        let max_length = LitInt::new(
                            &args.max_length.unwrap_or(30).to_string(),
                            proc_macro2::Span::call_site(),
                        );
                        let max_siblings = LitInt::new(
                            &args.max_siblings.unwrap_or(999).to_string(),
                            proc_macro2::Span::call_site(),
                        );
                        spec = quote! { #spec.with_limits(#max_length, #max_siblings) };
                    }
                    quote! {
                        /// 排序元信息：表名、外层隔离列与层级/同级上限
                        const SORT_SPEC: ::robotech::dao::sort::TreeSort = #spec;
                    }
                }
            };
            (generated, quote! { SORT_SPEC })
        }
    };

    let hooks = match args.kind.as_str() {
        "flat" => flat_hooks(
            &add_dto,
            &modify_dto,
            &query_dto,
            &vo,
            sort_field,
            group_field,
            spec_path,
        ),
        _ => tree_hooks(
            &add_dto,
            &modify_dto,
            &query_dto,
            &vo,
            code_field,
            parent_field,
            scope_field,
            spec_path,
        ),
    };

    // 生成 svc 主体：`#[svc]` 生成的 impl 里会按名字调用上面这 4 个钩子。
    // 注意属性宏无法再展开另一个属性宏，这里直接复用 svc 宏的代码生成。
    let svc_args: crate::svc::SvcArgs = match syn::parse2(quote! {
        before_add = sort_hooks_add,
        before_modify = sort_hooks_modify,
        after_del_by_id = sort_hooks_after_del_by_id,
        after_del_by_query_dto = sort_hooks_after_del_by_query_dto
    }) {
        Ok(args) => args,
        Err(err) => return err.to_compile_error(),
    };
    let svc_item = crate::svc::svc_macro(svc_args, input);

    quote! {
        #spec_def
        #hooks
        #svc_item
    }
}

/// 平铺实体的四个钩子
fn flat_hooks(
    add_dto: &Ident,
    modify_dto: &Ident,
    query_dto: &Ident,
    vo: &Ident,
    sort_field: Ident,
    group_field: Option<Ident>,
    spec_path: TokenStream,
) -> TokenStream {
    let has_group = group_field.is_some();
    let add_group = match &group_field {
        Some(field) => quote! {
            let group: Option<i64> =
                ::robotech::dao::sort::SortGroupValue::sort_group(dto.#field);
        },
        None => quote! { let group: Option<i64> = None; },
    };
    let modify_group = match &group_field {
        Some(field) => quote! {
            let new_group: Option<Option<i64>> = dto.#field
                .map(::robotech::dao::sort::SortGroupValue::sort_group);
        },
        None => quote! { let new_group: Option<Option<i64>> = None; },
    };
    let vo_group = match &group_field {
        Some(field) => quote! {
            let group: Option<i64> =
                ::robotech::dao::sort::SortGroupValue::sort_group(one.#field);
        },
        None => quote! { let group: Option<i64> = None; },
    };

    quote! {
        #[allow(clippy::result_large_err)]
        async fn sort_hooks_add<C>(
            mut dto: #add_dto,
            db: &C,
        ) -> Result<#add_dto, ::robotech::svc::SvcError>
        where
            C: ::sea_orm::ConnectionTrait + Sync,
        {
            #add_group
            dto.#sort_field = Some(
                ::robotech::dao::sort::next_sort_order(db, &#spec_path, group).await?,
            );
            Ok(dto)
        }

        #[allow(clippy::result_large_err)]
        async fn sort_hooks_modify<C>(
            mut dto: #modify_dto,
            db: &C,
        ) -> Result<#modify_dto, ::robotech::svc::SvcError>
        where
            C: ::sea_orm::ConnectionTrait + Sync,
        {
            // 排序值一律由服务端接管
            dto.#sort_field = None;
            #modify_group
            if let (Some(new_group), Some(id)) = (new_group, dto.id) {
                if #has_group {
                    let operator = {
                        let value = dto._current_user_id.value();
                        if value == 0 { None } else { Some(value) }
                    };
                    if let Some(next) = ::robotech::dao::sort::flat_move_group(
                        db,
                        &#spec_path,
                        id.value(),
                        new_group,
                        operator,
                    )
                    .await?
                    {
                        dto.#sort_field = Some(next);
                    }
                }
            }
            Ok(dto)
        }

        #[allow(clippy::result_large_err)]
        async fn sort_hooks_after_del_by_id<C>(
            _id: ::robotech::api::U64,
            one: #vo,
            db: &C,
        ) -> Result<#vo, ::robotech::svc::SvcError>
        where
            C: ::sea_orm::ConnectionTrait + Sync,
        {
            #vo_group
            ::robotech::dao::sort::compact_flat_after_delete(
                db,
                &#spec_path,
                group,
                one.#sort_field,
                None,
            )
            .await?;
            Ok(one)
        }

        #[allow(clippy::result_large_err)]
        async fn sort_hooks_after_del_by_query_dto<C>(
            _dto: #query_dto,
            rows_affected: u64,
            db: &C,
        ) -> Result<u64, ::robotech::svc::SvcError>
        where
            C: ::sea_orm::ConnectionTrait + Sync,
        {
            ::robotech::dao::sort::compact_flat_gaps(db, &#spec_path).await?;
            Ok(rows_affected)
        }
    }
}

/// 树形实体的四个钩子
fn tree_hooks(
    add_dto: &Ident,
    modify_dto: &Ident,
    query_dto: &Ident,
    vo: &Ident,
    code_field: Ident,
    parent_field: Ident,
    scope_field: Option<Ident>,
    spec_path: TokenStream,
) -> TokenStream {
    let add_scope = match &scope_field {
        Some(field) => quote! {
            let scope: Option<i64> =
                ::robotech::dao::sort::SortGroupValue::sort_group(dto.#field);
            // 外层范围必填时，缺了就不生成编码，交给 DTO 校验报「必填」
            if scope.is_none() {
                return Ok(dto);
            }
        },
        None => quote! { let scope: Option<i64> = None; },
    };
    let modify_scope = match &scope_field {
        Some(field) => quote! {
            let new_scope: Option<i64> =
                ::robotech::dao::sort::SortGroupValue::sort_group(dto.#field);
        },
        None => quote! { let new_scope: Option<i64> = None; },
    };
    let vo_scope = match &scope_field {
        Some(field) => quote! {
            let scope: Option<i64> =
                ::robotech::dao::sort::SortGroupValue::sort_group(one.#field);
        },
        None => quote! { let scope: Option<i64> = None; },
    };

    quote! {
        #[allow(clippy::result_large_err)]
        async fn sort_hooks_add<C>(
            mut dto: #add_dto,
            db: &C,
        ) -> Result<#add_dto, ::robotech::svc::SvcError>
        where
            C: ::sea_orm::ConnectionTrait + Sync,
        {
            #add_scope
            ::robotech::dao::sort::tree_lock(db, &#spec_path).await?;
            // AddDto 的父编码是 Option<Option<String>>：未传或空都表示根
            let parent: String = dto.#parent_field.clone().flatten().unwrap_or_default();
            let code =
                ::robotech::dao::sort::next_child_code(db, &#spec_path, scope, &parent).await?;
            dto.#code_field = Some(code);
            Ok(dto)
        }

        #[allow(clippy::result_large_err)]
        async fn sort_hooks_modify<C>(
            mut dto: #modify_dto,
            db: &C,
        ) -> Result<#modify_dto, ::robotech::svc::SvcError>
        where
            C: ::sea_orm::ConnectionTrait + Sync,
        {
            // 树编码代表位置，客户端传值一律忽略
            dto.#code_field = None;
            let Some(id) = dto.id else {
                return Ok(dto);
            };
            #modify_scope
            // `Some(None)` 明确移到根（空串），`None` 表示不改父节点
            let new_parent: Option<String> = dto.#parent_field
                .as_ref()
                .map(|value| value.clone().unwrap_or_default());
            if new_scope.is_none() && new_parent.is_none() {
                return Ok(dto);
            }
            let operator = {
                let value = dto._current_user_id.value();
                if value == 0 { None } else { Some(value) }
            };
            ::robotech::dao::sort::tree_move_node(
                db,
                ::robotech::dao::sort::TreeMoveArgs {
                    spec: &#spec_path,
                    id: id.value(),
                    new_scope,
                    new_parent,
                    operator_id: operator,
                },
            )
            .await?;
            Ok(dto)
        }

        #[allow(clippy::result_large_err)]
        async fn sort_hooks_after_del_by_id<C>(
            _id: ::robotech::api::U64,
            one: #vo,
            db: &C,
        ) -> Result<#vo, ::robotech::svc::SvcError>
        where
            C: ::sea_orm::ConnectionTrait + Sync,
        {
            #vo_scope
            ::robotech::dao::sort::compact_tree_after_delete(
                db,
                &#spec_path,
                scope,
                &one.#code_field,
                None,
            )
            .await?;
            Ok(one)
        }

        #[allow(clippy::result_large_err)]
        async fn sort_hooks_after_del_by_query_dto<C>(
            _dto: #query_dto,
            rows_affected: u64,
            db: &C,
        ) -> Result<u64, ::robotech::svc::SvcError>
        where
            C: ::sea_orm::ConnectionTrait + Sync,
        {
            ::robotech::dao::sort::compact_tree_gaps(db, &#spec_path).await?;
            Ok(rows_affected)
        }
    }
}
