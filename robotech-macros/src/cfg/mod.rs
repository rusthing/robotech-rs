use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Block, Expr, Token};

pub(super) struct WatchCfgFileArgs {
    title: String,
    files: Expr,
    reload_block: Block,
}

impl Parse for WatchCfgFileArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let title = input.parse::<syn::LitStr>()?.value();
        let _: Token![,] = input.parse()?;

        let files = input.parse::<Expr>()?;

        let _: Token![,] = input.parse()?;
        let reload_block = input.parse()?;

        Ok(WatchCfgFileArgs {
            title,
            files,
            reload_block,
        })
    }
}

pub(super) fn watch_cfg_file_macro(args: WatchCfgFileArgs) -> TokenStream {
    let WatchCfgFileArgs {
        title,
        files,
        reload_block,
    } = args;
    let reload_block = &reload_block.stmts;

    let expanded = quote! {
        use notify_debouncer_mini::DebouncedEventKind;


        log::debug!("watch {} cfg file: {:?} ...", #title, #files);
        tokio::spawn({
            async move {
                let (_watcher, receiver) = watch_cfg_file(#files).expect(&format!("watch {} cfg file error: {:?}", #title, #files));

                // 创建一个1秒间隔的定时器
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    // 等待下一个时间点
                    interval.tick().await;
                    // 使用 try_recv 非阻塞检查
                    match receiver.try_recv() {
                        Ok(event_result) => {
                            match event_result {
                                Ok(events) => {
                                    // 处理文件事件
                                    for event in events {
                                        log::trace!("{} cfg file change event: {:?} {:?}", #title, event, #files);
                                        if event.kind == DebouncedEventKind::AnyContinuous {
                                            // 事件持续发生，防抖超时了
                                            continue;
                                        }
                                    }
                                    log::debug!("reload from {} cfg file: {:?} ...", #title, #files);

                                    #( #reload_block )*
                                }
                                Err(e) => {
                                    log::warn!("error receiving {} cfg file events: {:?} {:?}", #title, #files, e);
                                }
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // 没有消息，继续下一次循环
                            continue;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            // 通道关闭
                            log::debug!("{} cfg file watcher channel closed, exiting watcher loop: {:?}", #title, #files);
                            break;
                        }
                    }
                }

                log::debug!("{} cfg file watcher task finished: {:?}", #title, #files);
            }
        });
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
