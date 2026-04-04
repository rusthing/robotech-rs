use crate::comm::SetArgsMode;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{ItemStruct, Token, bracketed, parenthesized};
use wheel_rs::str_utils::{CamelFormat, split_camel_case};

/// 唯一键字段配置项
#[derive(Debug)]
struct RouteArgs {
    value: TokenStream,
}

impl Parse for RouteArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        // 解开圆括号
        parenthesized!(content in input);

        Ok(Self {
            value: content.parse()?,
        })
    }
}

/// DAO方法生成宏参数解析
#[derive(Debug, Default)]
pub(crate) struct RouterArgs {
    add: bool,
    modify: bool,
    save: bool,
    del_by_id: bool,
    del_by_query_dto: bool,
    get_by_id: bool,
    get_by_query_dto: bool,
    list_by_query_dto: bool,
    page_by_query_dto: bool,
    routes: Vec<RouteArgs>,
}

impl RouterArgs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build_default() -> Self {
        Self {
            add: true,
            modify: true,
            save: true,
            del_by_id: true,
            del_by_query_dto: true,
            get_by_id: true,
            get_by_query_dto: true,
            list_by_query_dto: false,
            page_by_query_dto: true,
            routes: vec![],
        }
    }
    pub fn build_all() -> Self {
        Self {
            add: true,
            modify: true,
            save: true,
            del_by_id: true,
            del_by_query_dto: true,
            get_by_id: true,
            get_by_query_dto: true,
            list_by_query_dto: true,
            page_by_query_dto: true,
            routes: vec![],
        }
    }
}

impl Parse for RouterArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(RouterArgs::build_default());
        }

        let mut config_mode = SetArgsMode::Default;
        let mut routes = vec![];
        let mut result = RouterArgs::new();
        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            match ident.to_string().to_lowercase().as_str() {
                "none" => {
                    config_mode = SetArgsMode::None;
                }
                "default" => {
                    config_mode = SetArgsMode::Default;
                }
                "all" => {
                    config_mode = SetArgsMode::All;
                }
                "add" => {
                    config_mode = SetArgsMode::Custom;
                    result.add = true;
                }
                "modify" => {
                    config_mode = SetArgsMode::Custom;
                    result.modify = true;
                }
                "save" => {
                    config_mode = SetArgsMode::Custom;
                    result.save = true;
                }
                "del_by_id" => {
                    config_mode = SetArgsMode::Custom;
                    result.del_by_id = true;
                }
                "del_by_query_dto" => {
                    config_mode = SetArgsMode::Custom;
                    result.del_by_query_dto = true;
                }
                "get_by_id" => {
                    config_mode = SetArgsMode::Custom;
                    result.get_by_id = true;
                }
                "get_by_query_dto" => {
                    config_mode = SetArgsMode::Custom;
                    result.get_by_query_dto = true;
                }
                "list_by_query_dto" => {
                    config_mode = SetArgsMode::Custom;
                    result.list_by_query_dto = true;
                }
                "page_by_query_dto" => {
                    config_mode = SetArgsMode::Custom;
                    result.page_by_query_dto = true;
                }
                "routes" => {
                    // 解析 routes 后面的数组，数组元素是元组 (path, handler)
                    let content;
                    // 解开方括号
                    bracketed!(content in input);
                    // 解析逗号分隔的列表
                    let parsed_args = content.parse_terminated(RouteArgs::parse, Token![,])?;
                    routes = parsed_args.into_iter().collect();
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "Invalid argument, please check the parameter",
                    ));
                }
            }
            if let Err(_) = input.parse::<Token![,]>() {
                break;
            }
        }

        if config_mode == SetArgsMode::Default {
            result = RouterArgs::build_default();
        } else if config_mode == SetArgsMode::All {
            result = RouterArgs::build_all();
        }

        result.routes = routes;

        Ok(result)
    }
}

pub(crate) fn router_macro(args: RouterArgs, input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;

    // 解析结构体的名称，必须是Router结尾，符合大驼峰命名规范
    let struct_name_str = struct_name.to_string();
    if !struct_name_str.ends_with("Router") {
        return syn::Error::new_spanned(struct_name, "Struct name must end with 'Router'")
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
    let path = struct_name_split.join("/").to_lowercase();
    let ctrl_module = format_ident!("{module_name}_ctrl");
    let module_path = format!("/{path}");
    let crud_path = module_path.clone();
    let save_path = format!("{module_path}/save");
    let del_by_id_path = format!("{crud_path}/{{id}}");
    let del_by_query_dto_path = crud_path.clone();
    let get_by_id_path = format!("{crud_path}/{{id}}");
    let get_by_query_dto_path = crud_path.clone();
    let list_by_query_dto_path = format!("{module_path}/list");
    let page_by_query_dto_path = format!("{module_path}/page");
    let mut routes = vec![];

    if args.add {
        routes.push(quote! {#crud_path, post(add)});
    }
    if args.modify {
        routes.push(quote! {#crud_path, put(modify)});
    }
    if args.save {
        routes.push(quote! {#save_path, post(save)});
    }
    if args.del_by_id {
        routes.push(quote! {#del_by_id_path, delete(del_by_id)});
    }
    if args.del_by_query_dto {
        routes.push(quote! {#del_by_query_dto_path, delete(del_by_query_dto)});
    }
    if args.get_by_id {
        routes.push(quote! {#get_by_id_path, get(get_by_id)});
    }
    if args.get_by_query_dto {
        routes.push(quote! {#get_by_query_dto_path, get(get_by_query_dto)});
    }
    if args.list_by_query_dto {
        routes.push(quote! {#list_by_query_dto_path, get(list_by_query_dto)});
    }
    if args.page_by_query_dto {
        routes.push(quote! {#page_by_query_dto_path, get(page_by_query_dto)});
    }

    for route_args in &args.routes {
        routes.push(route_args.value.clone());
    }

    let expanded = quote! {
        use crate::web::ctrl::#ctrl_module::*;
        use axum::{
            routing::{delete, get, post, put},
            Router,
        };
        use linkme::distributed_slice;
        use robotech::web::INIT_ROUTERS_SLICE;

        #[distributed_slice(INIT_ROUTERS_SLICE)]
        static INIT_ROUTERS_FN: fn() -> Router = init_routes;

        fn init_routes() -> Router {
            Router::new()
            #(.route(#routes))*
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
