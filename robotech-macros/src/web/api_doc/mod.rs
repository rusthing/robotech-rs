use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemStruct, Token};
use wheel_rs::str_utils::{CamelFormat, split_camel_case};

/// ApiDoc方法生成宏参数解析
#[derive(Debug, Default)]
pub(crate) struct ApiDocArgs {
    paths: Vec<String>,
}

impl Parse for ApiDocArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(syn::Error::new(input.span(), "No arguments provided"));
        }

        let mut paths = Vec::new();

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            paths.push(ident.to_string());

            if input.parse::<Token![,]>().is_err() {
                break;
            }
        }

        Ok(ApiDocArgs { paths })
    }
}

pub(crate) fn api_doc_macro(args: ApiDocArgs, input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;

    // 解析结构体的名称，必须是ApiDoc结尾，符合大驼峰命名规范
    let struct_name_str = struct_name.to_string();
    if !struct_name_str.ends_with("ApiDoc") {
        return syn::Error::new_spanned(struct_name, "Struct name must end with 'ApiDoc'")
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
    let len = struct_name_split.len();
    struct_name_split.truncate(len - 2);
    let module_name = struct_name_split.join("_").to_lowercase();
    let url_path = struct_name_split.join("-").to_lowercase();
    let ctrl_module = format_ident!("{module_name}_ctrl");
    let url_path = format!("/{url_path}/openapi.json");

    let paths: Vec<Ident> = args.paths.iter().map(|p| format_ident!("{}", p)).collect();

    let expanded = quote! {
        use crate::web::ctrl::#ctrl_module::*;
        use linkme::distributed_slice;
        use robotech::web::API_DOC_SLICE;
        use utoipa::OpenApi;
        use utoipa_swagger_ui::Url;

        #[derive(OpenApi)]
        #[openapi(paths(#(#paths),*))]
        pub struct #struct_name;

        #[distributed_slice(API_DOC_SLICE)]
        static BUILD_API_DOC_FN: fn() -> (Url<'static>, utoipa::openapi::OpenApi) = build_api_doc;

        fn build_api_doc() -> (Url<'static>, utoipa::openapi::OpenApi) {
            (
                #url_path.into(),
                #struct_name::openapi(),
            )
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
