//! # db_unwrap 属性宏
//!
//! 实现 `#[db_unwrap]` 属性宏的参数解析与代码生成，为 Service 查询方法自动
//! 处理数据库连接获取与可选的事务开启逻辑。

use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{FnArg, ItemFn, Pat};

/// db_unwrap属性宏参数解析
#[derive(Debug, Default)]
pub(crate) struct DbUnwrapArgs {
    /// 需要事务
    transaction_required: bool,
}

impl Parse for DbUnwrapArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(DbUnwrapArgs::default());
        }

        let ident: Ident = input.parse()?;
        match ident.to_string().to_lowercase().as_str() {
            "transaction_required" => Ok(DbUnwrapArgs {
                transaction_required: true,
            }),
            unknown => Err(syn::Error::new_spanned(
                ident,
                format!("Unknown argument: {unknown}"),
            )),
        }
    }
}

pub(crate) fn db_unwrap_macro(args: DbUnwrapArgs, input: ItemFn) -> TokenStream {
    let fn_attrs = &input.attrs;
    let fn_vis = &input.vis;
    let fn_sig = &input.sig;

    let transaction_required = args.transaction_required;

    // 分析函数签名，提取参数和返回类型
    let has_db_param = input.sig.inputs.iter().any(|arg| match arg {
        FnArg::Typed(pat_type) => {
            if let Pat::Ident(pat_ident) = &*pat_type.pat {
                pat_ident.ident == "db"
            } else {
                false
            }
        }
        _ => false,
    });

    // 如果没有 db 参数，报错
    if !has_db_param {
        return syn::Error::new_spanned(
            &fn_sig,
            "Service query method must have a 'db: Option<&C>' parameter",
        )
        .to_compile_error()
        .into();
    }

    // 提取用户编写的代码块
    let user_block = &input.block;

    // 生成包装后的方法
    //
    // `transaction_required` 分支里事务必须**包住**用户块并在正常结束处提交：
    // 之前把 `let db = &tx;` 写在 `if` 块内部，遮蔽只在该块生效，用户块实际
    // 拿到的还是池连接——事务开了又空转着回滚，等于没有事务。
    //
    // 块内 `?` / `return` 会直接跳出函数，tx 随之 drop（即回滚），只有正常
    // 走到块尾才会提交。
    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig {
            if let Some(db) = db {
                #user_block
            } else {
                let db_conn = robotech::db::get_db_conn()?;
                if #transaction_required {
                    let tx = robotech::dao::begin_transaction(db_conn.as_ref()).await?;
                    let out = {
                        let db = &tx;
                        #user_block
                    };
                    robotech::dao::commit_transaction(tx).await?;
                    out
                } else {
                    let db = db_conn.as_ref();
                    #user_block
                }
            }
        }
    };

    TokenStream::from(expanded)
}
