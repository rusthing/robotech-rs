use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{FnArg, ItemFn, Pat, PatType, Token};

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
    /// 记录模式：进入、退出、两者都记录
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
    normalized.contains("Path<") || normalized.contains("Json<") || normalized.contains("Query<")
}

/// 检查参数是否带有 #[skip_log] 属性
fn has_skip_log(pat_type: &PatType) -> bool {
    pat_type
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("skip_log"))
}

pub(super) fn log_call_macro(args: LogCallArgs, mut input: ItemFn) -> TokenStream {
    let LogCallArgs {
        level: log_level,
        mode: record_mode,
    } = args;

    let fn_attrs = &input.attrs;
    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let fn_block = &input.block;
    let fn_vis = &input.vis;

    // ── 第一步：收集需要记录的参数，同时剥除所有 #[skip_log] 属性 ──────────────
    let mut param_formats = Vec::new();
    let mut param_values = Vec::new();

    for arg in &input.sig.inputs {
        match arg {
            FnArg::Typed(pat_type) => {
                // 跳过带 #[skip_log] 的参数，不收集其格式和值
                if has_skip_log(pat_type) {
                    continue;
                }

                let ty = &pat_type.ty;
                let type_str = quote!(#ty).to_string();
                let is_wrapper = is_axum_wrapper(&type_str);

                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    // 普通写法：path: Path<T>  或  val: MyType
                    let param_name = &pat_ident.ident;
                    let param_name_str = param_name.to_string();
                    param_formats.push(format!("{} = {{:?}}", param_name_str));
                    if is_wrapper {
                        param_values.push(quote! { #param_name.0 });
                    } else {
                        param_values.push(quote! { #param_name });
                    }
                } else if let Pat::TupleStruct(pat_ts) = &*pat_type.pat {
                    // 解构写法：Path(id): Path<u64>  /  Json(mut dto): Json<Dto>
                    // 必须用裸 Ident，避免 mut 被带入表达式位置
                    if pat_ts.elems.len() == 1 {
                        if let Pat::Ident(inner) = &pat_ts.elems[0] {
                            let inner_name = inner.ident.to_string();
                            param_formats.push(format!("{} = {{:?}}", inner_name));
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

    // ── 第二步：从函数签名中剥除所有 #[skip_log] 属性 ────────────────────────
    // 必须在生成代码之前完成，否则编译器会报"未知属性"错误
    for arg in input.sig.inputs.iter_mut() {
        if let FnArg::Typed(pat_type) = arg {
            pat_type
                .attrs
                .retain(|attr| !attr.path().is_ident("skip_log"));
        }
    }

    let fn_sig = &input.sig;

    // ── 第三步：生成日志代码 ──────────────────────────────────────────────────
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
        #(#fn_attrs)*
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
