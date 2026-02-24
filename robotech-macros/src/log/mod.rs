use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{FnArg, ItemFn, Pat, Token};

/// 宏参数解析结构
pub(super) struct LogCallArgs {
    level: Option<Ident>,
}

impl Parse for LogCallArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // 如果输入为空，返回 None
        if input.is_empty() {
            return Ok(LogCallArgs { level: None });
        }

        // 解析 level = xxx 的形式
        let _level_key: Ident = input.parse()?;
        let _: Token![=] = input.parse()?;
        let level: Ident = input.parse()?;

        Ok(LogCallArgs { level: Some(level) })
    }
}

pub(super) fn log_call_macro(args: LogCallArgs, input: ItemFn) -> TokenStream {
    // 如果没有指定 level，默认使用 debug
    let log_level = args.level.unwrap_or_else(|| format_ident!("debug"));

    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let fn_block = &input.block;
    let fn_vis = &input.vis;
    let fn_sig = &input.sig;

    // 收集参数信息
    let mut param_formats = Vec::new();
    let mut param_values = Vec::new();

    for arg in &input.sig.inputs {
        match arg {
            FnArg::Typed(pat_type) => {
                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    let param_name = &pat_ident.ident;
                    let param_name_str = param_name.to_string();

                    param_formats.push(format!("  {} = {{:?}}", param_name_str));
                    param_values.push(quote! { #param_name });
                }
            }
            FnArg::Receiver(_) => {
                param_formats.push("  self = {:?}".to_string());
                param_values.push(quote! { self });
            }
        }
    }

    // 构建新的函数体
    let expanded = if param_formats.is_empty() {
        // 没有参数的情况
        quote! {
            #fn_vis #fn_sig {
                log::#log_level!("→ 进入方法: {}()", #fn_name_str);
                #fn_block
            }
        }
    } else {
        // 有参数的情况 - 构建完整的格式字符串
        let format_string = format!(
            "→ 进入方法: {}() 参数: \n{}",
            fn_name_str,
            param_formats.join("\n")
        );

        quote! {
            #fn_vis #fn_sig {
                log::#log_level!(#format_string, #(#param_values),*);
                #fn_block
            }
        }
    };

    TokenStream::from(expanded)
}
