use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{ItemStruct, Token, bracketed, parenthesized};
use wheel_rs::str_utils::{CamelFormat, split_camel_case};

/// Routes字段配置项
#[derive(Debug)]
struct RoutesArgs {
    value: TokenStream,
}

impl Parse for RoutesArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        // 解开圆括号
        parenthesized!(content in input);

        Ok(Self {
            value: content.parse()?,
        })
    }
}

/// router方法生成宏参数解析
#[derive(Debug, Default)]
pub(crate) struct RouterArgs {
    crud: bool,
    routes: Vec<RoutesArgs>,
}

impl Parse for RouterArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(RouterArgs::default());
        }

        let mut result = RouterArgs::default();
        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            match ident.to_string().to_lowercase().as_str() {
                "crud" => {
                    result.crud = true;
                }
                "routes" => {
                    // 解析 routes 后面的数组，数组元素是元组 (path, handler)
                    let content;
                    // 解开方括号
                    bracketed!(content in input);
                    // 解析逗号分隔的列表
                    let parsed_args = content.parse_terminated(RoutesArgs::parse, Token![,])?;
                    result.routes = parsed_args.into_iter().collect();
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
    let ctrl_module = format_ident!("{module_name}_ctrl");
    let prefix = struct_name_split.remove(0).to_lowercase();
    let module_path = struct_name_split.join("-").to_lowercase();
    let crud_path = format!("/{prefix}/{module_path}");
    let save_path = format!("{crud_path}/save");
    let del_by_id_path = format!("{crud_path}/{{id}}");
    let del_by_query_dto_path = crud_path.clone();
    let get_by_id_path = format!("{crud_path}/{{id}}");
    let get_by_query_dto_path = crud_path.clone();
    let list_by_query_dto_path = format!("{crud_path}/list");
    let page_by_query_dto_path = format!("{crud_path}/page");
    let mut routes = vec![];

    for route_args in &args.routes {
        routes.push(route_args.value.clone());
    }

    if args.crud {
        routes.push(quote! {#crud_path, post(add)});
        routes.push(quote! {#crud_path, put(modify)});
        routes.push(quote! {#save_path, post(save)});
        routes.push(quote! {#del_by_id_path, delete(del_by_id)});
        routes.push(quote! {#del_by_query_dto_path, delete(del_by_query_dto)});
        routes.push(quote! {#get_by_query_dto_path, get(get_by_query_dto)});
        routes.push(quote! {#list_by_query_dto_path, get(list_by_query_dto)});
        routes.push(quote! {#page_by_query_dto_path, get(page_by_query_dto)});
        routes.push(quote! {#get_by_id_path, get(get_by_id)}); // 这个放在后面，避免覆盖前面的list和page
    }

    let expanded = quote! {
        use crate::web::ctrl::#ctrl_module::*;
        use axum::{
            routing::{delete, get, post, put},
            Router,
        };
        use linkme::distributed_slice;
        use robotech::web::ROUTER_SLICE;

        #[distributed_slice(ROUTER_SLICE)]
        static BUILD_ROUTER_FN: fn() -> Router = build_router;

        fn build_router() -> Router {
            Router::new()
            #(.route(#routes))*
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
