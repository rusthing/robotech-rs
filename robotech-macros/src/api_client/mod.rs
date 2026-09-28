//! # API Client 聚合属性宏
//!
//! 实现 `#[api_client]` 属性宏，为聚合多个 XxxApiClient 的结构体生成 static、getter 和 setup 函数。

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Fields, ItemStruct};
use wheel_rs::str_utils::{split_camel_case, CamelFormat};

pub(crate) fn api_client_macro(input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;
    let struct_name_str = struct_name.to_string();

    if !struct_name_str.ends_with("ApiClient") {
        return syn::Error::new_spanned(struct_name, "Struct name must end with 'ApiClient'")
            .to_compile_error()
            .into();
    }

    let mut parts = match split_camel_case(&struct_name_str, CamelFormat::Upper) {
        Ok(parts) => parts,
        Err(e) => return syn::Error::new_spanned(struct_name, e).to_compile_error().into(),
    };

    // strip "Api" and "Client"
    parts.pop();
    parts.pop();
    let config_key = parts.join("").to_lowercase();
    let getter_name = format_ident!("get_{config_key}_api_client");
    let setup_name = format_ident!("setup_{config_key}_api_client");

    let Fields::Named(fields) = &input.fields else {
        return syn::Error::new_spanned(
            struct_name,
            "api_client struct must have named fields",
        )
        .to_compile_error()
        .into();
    };

    let field_inits: Vec<_> = fields
        .named
        .iter()
        .map(|f| {
            let name = &f.ident;
            let ty = &f.ty;
            quote! {
                let #name = <#ty>::new(
                    robotech::micro_svc::FeignApiClient::new(cfg.clone()).await
                );
            }
        })
        .collect();

    let field_names: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();

    let expanded = quote! {
        #input

        static API_CLIENT: arc_swap::ArcSwapOption<#struct_name> = arc_swap::ArcSwapOption::const_empty();

        const API_CLIENT_CONFIG_KEY: &str = #config_key;

        pub fn #getter_name() -> ::core::result::Result<
            ::std::sync::Arc<#struct_name>,
            robotech::cfg::CfgError,
        > {
            API_CLIENT.load_full().ok_or(robotech::cfg::CfgError::NotInit(
                concat!(stringify!(#struct_name), " not initialized").to_string(),
            ))
        }

        pub async fn #setup_name(
            apis_config: ::std::collections::HashMap<
                String,
                robotech::api_client::ApiClientConfig,
            >,
            changed: &::core::option::Option<
                ::std::collections::HashMap<String, config::Value>,
            >,
        ) -> ::core::result::Result<(), robotech::cfg::CfgError> {
            use robotech::api_client::API_CLIENT_CONFIG_KEY_PREFIX;
            use tracing::info;

            info!("setup {} api client...: {:?} {:?}", #config_key, apis_config, changed);

            let key_prefix = ::std::format!(
                "{}.{}",
                API_CLIENT_CONFIG_KEY_PREFIX,
                API_CLIENT_CONFIG_KEY
            );
            if changed
                .as_ref()
                .map(|c| wheel_rs::config_utils::has_config_changed(&key_prefix, c))
                .unwrap_or(true)
            {
                let cfg = apis_config.get(API_CLIENT_CONFIG_KEY).ok_or_else(|| {
                    robotech::cfg::CfgError::NotInit(
                        ::std::format!("{} not found in apis_config", API_CLIENT_CONFIG_KEY),
                    )
                })?;

                #(#field_inits)*

                API_CLIENT.store(::core::option::Option::Some(::std::sync::Arc::new(
                    #struct_name { #(#field_names),* },
                )));
            }
            Ok(())
        }
    };

    TokenStream::from(expanded)
}