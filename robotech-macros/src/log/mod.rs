use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{FnArg, ItemFn, Pat, Token};

#[derive(PartialEq)]
enum RecordMode {
    Enter,
    Exit,
    Both,
}

/// 宏参数解析结构
pub(super) struct LogCallArgs {
    /// 日志级别
    level: Ident,
    /// 记录模式
    /// 进入、退出、两者都记录
    mode: RecordMode,
}

impl Parse for LogCallArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // 如果输入为空，返回 None
        if input.is_empty() {
            return Ok(LogCallArgs {
                level: format_ident!("debug"),
                mode: RecordMode::Both,
            });
        }

        // 解析 level = xxx, mode = yyy 的形式
        let _level_key: Ident = input.parse()?;
        let _: Token![=] = input.parse()?;
        let level: Ident = input.parse()?;

        // 尝试解析逗号和 mode 参数（可选）
        let mode = if input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            let mode_key: Ident = input.parse()?;
            let _: Token![=] = input.parse()?;
            let mode_ident: Ident = input.parse()?;
            match mode_ident.to_string().to_lowercase().as_str() {
                "enter" => RecordMode::Enter,
                "exit" => RecordMode::Exit,
                "both" => RecordMode::Both,
                _ => {
                    return Err(syn::Error::new_spanned(mode_key, "无效的 mode 参数"));
                }
            }
        } else {
            RecordMode::Both
        };

        Ok(LogCallArgs { level, mode })
    }
}

pub(super) fn log_call_macro(args: LogCallArgs, input: ItemFn) -> TokenStream {
    let LogCallArgs {
        level: log_level,
        mode: record_mode,
    } = args;

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

                    param_formats.push(format!("{} = {{:?}}", param_name_str));
                    param_values.push(quote! { #param_name });
                }
            }
            FnArg::Receiver(_) => {
                param_formats.push("self = {:?}".to_string());
                param_values.push(quote! { self });
            }
        }
    }

    let enter_log = format!(
        "进入方法 ➡️ {fn_name_str}{}",
        if param_formats.is_empty() {
            "()".to_string()
        } else {
            format!("({})", param_formats.join(", "))
        }
    );
    let enter_log = if record_mode == RecordMode::Both || record_mode == RecordMode::Enter {
        quote! {
            log::#log_level!(#enter_log, #(#param_values),*);
        }
    } else {
        quote! {}
    };
    let exit_log = if record_mode == RecordMode::Both || record_mode == RecordMode::Exit {
        quote! {
            log::#log_level!("退出方法 ↩️ {}(), 返回值: {:?}", #fn_name_str, result);
        }
    } else {
        quote! {}
    };

    // 构建新的函数体
    let expanded = quote! {
        #fn_vis #fn_sig {
            #enter_log
            let result = #fn_block;
            #exit_log
            result
        }
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
