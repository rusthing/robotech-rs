use proc_macro2::{Ident, Span, TokenStream};
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
        if input.is_empty() {
            return Ok(LogCallArgs {
                level: format_ident!("debug"),
                mode: RecordMode::Both,
            });
        }

        let _level_key: Ident = input.parse()?;
        let _: Token![=] = input.parse()?;
        let level: Ident = input.parse()?;

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

/// 判断类型字符串是否为 axum 的 extractor 包装器（Path<T> / Json<T> / Query<T>）
/// 类型名后必须紧跟 '<'，避免误匹配 PathBuf 等类型
fn is_axum_wrapper(type_str: &str) -> bool {
    let normalized = type_str.replace(' ', "");
    normalized.contains("Path<")
        || normalized.contains("Json<")
        || normalized.contains("Query<")
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

    let mut param_formats = Vec::new();
    let mut param_values = Vec::new();

    for arg in &input.sig.inputs {
        match arg {
            FnArg::Typed(pat_type) => {
                let ty = &pat_type.ty;
                let type_str = quote!(#ty).to_string();
                let is_wrapper = is_axum_wrapper(&type_str);

                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    // 普通写法：path: Path<T>  或  val: MyType
                    let param_name = &pat_ident.ident;
                    let param_name_str = param_name.to_string();
                    param_formats.push(format!("{} = {{:?}}", param_name_str));
                    if is_wrapper {
                        // path: Path<T> → path.0 取出内部值
                        param_values.push(quote! { #param_name.0 });
                    } else {
                        param_values.push(quote! { #param_name });
                    }
                } else if let Pat::TupleStruct(pat_ts) = &*pat_type.pat {
                    // 解构写法：Path(id): Path<u64>  /  Json(mut dto): Json<Dto>
                    // 注意：内部可能带 mut（如 Json(mut dto)），必须剥离 mut 只取 Ident
                    // 否则 quote! { #inner } 会生成 `mut dto`，在表达式位置非法
                    if pat_ts.elems.len() == 1 {
                        if let Pat::Ident(inner) = &pat_ts.elems[0] {
                            let inner_name = inner.ident.to_string();
                            param_formats.push(format!("{} = {{:?}}", inner_name));
                            // 剥离 mut：只使用纯 Ident，不携带 mutability
                            let bare_ident = Ident::new(&inner_name, Span::call_site());
                            param_values.push(quote! { #bare_ident });
                        }
                    }
                } else if let Pat::Tuple(pat_tuple) = &*pat_type.pat {
                    // 单元素元组解构：(id,): Path<u64>（较少见）
                    if pat_tuple.elems.len() == 1 {
                        if let Pat::Ident(pat_ident) = &pat_tuple.elems[0] {
                            let inner_name = pat_ident.ident.to_string();
                            param_formats.push(format!("{} = {{:?}}", inner_name));
                            let bare_ident = Ident::new(&inner_name, Span::call_site());
                            if is_wrapper {
                                param_values.push(quote! { #bare_ident.0 });
                            } else {
                                param_values.push(quote! { #bare_ident });
                            }
                        }
                    }
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