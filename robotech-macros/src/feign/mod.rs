//! # Feign 客户端属性宏
//!
//! 实现 `#[feign_client]` 属性宏的代码生成，为包装了 `FeignApiClient` 的结构体
//! 生成 `build_headers` 辅助方法及一组 CRUD 请求方法。

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ItemStruct;
use wheel_rs::str_utils::{split_camel_case, CamelFormat};

pub(crate) fn feign_macro(input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;

    let struct_name_str = struct_name.to_string();
    if !struct_name_str.ends_with("ApiClient") {
        return syn::Error::new_spanned(struct_name, "Struct name must end with 'ApiClient'")
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

    // pop "Client" and "Api" to get the entity name
    struct_name_split.pop();
    struct_name_split.pop();

    let entity_name = struct_name_split.join("");
    let prefix = struct_name_split.remove(0).to_lowercase();
    let module_path = struct_name_split.join("-").to_lowercase();
    let crud_path = format!("/{prefix}/{module_path}");
    let save_path = format!("{crud_path}/save");
    let del_by_id_path = format!("{crud_path}/{{}}");
    let del_by_query_dto_path = crud_path.clone();
    let get_by_id_path = format!("{crud_path}/{{}}");
    let get_by_query_dto_path = crud_path.clone();
    let list_by_query_dto_path = format!("{crud_path}/list");
    let page_by_query_dto_path = format!("{crud_path}/page");
    let get_ex_by_id_path = format!("{crud_path}/ex/{{}}");
    let get_ex_by_query_dto_path = format!("{crud_path}/ex");
    let list_ex_by_query_dto_path = format!("{crud_path}/list-ex");
    let page_ex_by_query_dto_path = format!("{crud_path}/page-ex");
    let vo_name = format_ident!("{}Vo", entity_name);
    let ex_vo_name = format_ident!("{}ExVo", entity_name);
    let add_dto_name = format_ident!("{}AddDto", entity_name);
    let modify_dto_name = format_ident!("{}ModifyDto", entity_name);
    let save_dto_name = format_ident!("{}SaveDto", entity_name);
    let query_dto_name = format_ident!("{}QueryDto", entity_name);

    let mut generated_methods = Vec::new();

    // add
    generated_methods.push(quote! {
        pub async fn add(
            &self,
            dto: &#add_dto_name,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<#vo_name>, robotech::api_client::ApiClientError> {
            let url = #crud_path.to_string();
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<#add_dto_name, #vo_name>(
                    ::reqwest::Method::POST,
                    &url,
                    ::core::option::Option::None,
                    ::core::option::Option::Some(dto),
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // modify
    generated_methods.push(quote! {
        pub async fn modify(
            &self,
            dto: &#modify_dto_name,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<#vo_name>, robotech::api_client::ApiClientError> {
            let url = #crud_path.to_string();
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<#modify_dto_name, #vo_name>(
                    ::reqwest::Method::PUT,
                    &url,
                    ::core::option::Option::None,
                    ::core::option::Option::Some(dto),
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // save
    generated_methods.push(quote! {
        pub async fn save(
            &self,
            dto: &#save_dto_name,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<#vo_name>, robotech::api_client::ApiClientError> {
            let url = #save_path.to_string();
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<#save_dto_name, #vo_name>(
                    ::reqwest::Method::POST,
                    &url,
                    ::core::option::Option::None,
                    ::core::option::Option::Some(dto),
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // del_by_id
    generated_methods.push(quote! {
        pub async fn del_by_id(
            &self,
            id: u64,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<#vo_name>, robotech::api_client::ApiClientError> {
            let url = ::std::format!(#del_by_id_path, id);
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<(), #vo_name>(
                    ::reqwest::Method::DELETE,
                    &url,
                    ::core::option::Option::None::<&()>,
                    ::core::option::Option::None::<&()>,
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // del_by_query_dto
    generated_methods.push(quote! {
        pub async fn del_by_query_dto(
            &self,
            dto: &#query_dto_name,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<::serde_json::Value>, robotech::api_client::ApiClientError> {
            let url = #del_by_query_dto_path.to_string();
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<#query_dto_name, ::serde_json::Value>(
                    ::reqwest::Method::DELETE,
                    &url,
                    ::core::option::Option::Some(dto),
                    ::core::option::Option::None::<&#query_dto_name>,
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // get_by_id
    generated_methods.push(quote! {
        pub async fn get_by_id(
            &self,
            id: u64,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<#vo_name>, robotech::api_client::ApiClientError> {
            let url = ::std::format!(#get_by_id_path, id);
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<(), #vo_name>(
                    ::reqwest::Method::GET,
                    &url,
                    ::core::option::Option::None::<&()>,
                    ::core::option::Option::None::<&()>,
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // get_by_query_dto
    generated_methods.push(quote! {
        pub async fn get_by_query_dto(
            &self,
            dto: &#query_dto_name,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<#vo_name>, robotech::api_client::ApiClientError> {
            let url = #get_by_query_dto_path.to_string();
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<#query_dto_name, #vo_name>(
                    ::reqwest::Method::GET,
                    &url,
                    ::core::option::Option::Some(dto),
                    ::core::option::Option::None::<&#query_dto_name>,
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // list_by_query_dto
    generated_methods.push(quote! {
        pub async fn list_by_query_dto(
            &self,
            dto: &#query_dto_name,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<::std::vec::Vec<#vo_name>>, robotech::api_client::ApiClientError> {
            let url = #list_by_query_dto_path.to_string();
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<#query_dto_name, ::std::vec::Vec<#vo_name>>(
                    ::reqwest::Method::GET,
                    &url,
                    ::core::option::Option::Some(dto),
                    ::core::option::Option::None::<&#query_dto_name>,
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // page_by_query_dto
    generated_methods.push(quote! {
        pub async fn page_by_query_dto(
            &self,
            dto: &#query_dto_name,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<robotech::api::rx::PageRx<#vo_name>>, robotech::api_client::ApiClientError> {
            let url = #page_by_query_dto_path.to_string();
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<#query_dto_name, robotech::api::rx::PageRx<#vo_name>>(
                    ::reqwest::Method::GET,
                    &url,
                    ::core::option::Option::Some(dto),
                    ::core::option::Option::None::<&#query_dto_name>,
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // get_ex_by_id
    generated_methods.push(quote! {
        pub async fn get_ex_by_id(
            &self,
            id: u64,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<#ex_vo_name>, robotech::api_client::ApiClientError> {
            let url = ::std::format!(#get_ex_by_id_path, id);
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<(), #ex_vo_name>(
                    ::reqwest::Method::GET,
                    &url,
                    ::core::option::Option::None::<&()>,
                    ::core::option::Option::None::<&()>,
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // get_ex_by_query_dto
    generated_methods.push(quote! {
        pub async fn get_ex_by_query_dto(
            &self,
            dto: &#query_dto_name,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<#ex_vo_name>, robotech::api_client::ApiClientError> {
            let url = #get_ex_by_query_dto_path.to_string();
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<#query_dto_name, #ex_vo_name>(
                    ::reqwest::Method::GET,
                    &url,
                    ::core::option::Option::Some(dto),
                    ::core::option::Option::None::<&#query_dto_name>,
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // list_ex_by_query_dto
    generated_methods.push(quote! {
        pub async fn list_ex_by_query_dto(
            &self,
            dto: &#query_dto_name,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<::std::vec::Vec<#ex_vo_name>>, robotech::api_client::ApiClientError> {
            let url = #list_ex_by_query_dto_path.to_string();
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<#query_dto_name, ::std::vec::Vec<#ex_vo_name>>(
                    ::reqwest::Method::GET,
                    &url,
                    ::core::option::Option::Some(dto),
                    ::core::option::Option::None::<&#query_dto_name>,
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    // page_ex_by_query_dto
    generated_methods.push(quote! {
        pub async fn page_ex_by_query_dto(
            &self,
            dto: &#query_dto_name,
            current_user_id: u64,
        ) -> ::core::result::Result<robotech::api::Ro<robotech::api::rx::PageRx<#ex_vo_name>>, robotech::api_client::ApiClientError> {
            let url = #page_ex_by_query_dto_path.to_string();
            let headers = Self::build_headers(current_user_id)?;
            self.client
                .request::<#query_dto_name, robotech::api::rx::PageRx<#ex_vo_name>>(
                    ::reqwest::Method::GET,
                    &url,
                    ::core::option::Option::Some(dto),
                    ::core::option::Option::None::<&#query_dto_name>,
                    ::core::option::Option::Some(&headers),
                )
                .await
        }
    });

    let expanded = quote! {
        #input

        impl #struct_name {
            fn build_headers(
                current_user_id: u64,
            ) -> ::core::result::Result<
                ::reqwest::header::HeaderMap,
                robotech::api_client::ApiClientError,
            > {
                let mut headers = ::reqwest::header::HeaderMap::new();
                headers.insert(
                    robotech::cst::user_id_cst::USER_ID_HEADER_NAME,
                    ::reqwest::header::HeaderValue::from_str(
                        &current_user_id.to_string().as_str(),
                    )
                    .map_err(|e| ::anyhow::anyhow!("current_user_id: {}", e))?,
                );
                Ok(headers)
            }

            #(#generated_methods)*
        }
    };

    TokenStream::from(expanded)
}