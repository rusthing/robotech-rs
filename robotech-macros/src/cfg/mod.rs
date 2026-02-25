use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Block, Token};

pub(super) struct WatchCfgFileArgs {
    title: String,
    clone_block: Block,
    reload_block: Block,
}

impl Parse for WatchCfgFileArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let title = input.parse::<syn::LitStr>()?.value();
        let _: Token![,] = input.parse()?;
        let clone_block = input.parse()?;
        let _: Token![,] = input.parse()?;
        let reload_block = input.parse()?;

        Ok(WatchCfgFileArgs {
            title,
            clone_block,
            reload_block,
        })
    }
}

pub(super) fn watch_cfg_file_macro(input: WatchCfgFileArgs) -> TokenStream {
    let WatchCfgFileArgs {
        title,
        clone_block,
        reload_block,
    } = input;

    let clone_block = &clone_block.stmts;
    let reload_block = &reload_block.stmts;

    let expanded = quote! {
        debug!("watch {} cfg file...", #title);
        tokio::spawn({
            #( #clone_block )*
            async move {
                let (_watcher, receiver) = watch_cfg_file(files).expect(&format!("watch {} cfg file error", #title));

                // 创建一个1秒间隔的定时器
                let mut interval = interval(Duration::from_secs(1));
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
                                        debug!("{} cfg file change event: {:?}", #title, event);
                                    }
                                    debug!("reload from {} cfg file...", #title);

                                    #( #reload_block )*
                                }
                                Err(e) => {
                                    warn!("error receiving {} cfg file events: {:?}", #title, e);
                                }
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => {
                            // 没有消息，继续下一次循环
                            continue;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            // 通道关闭
                            debug!("{} cfg file watcher channel closed, exiting watcher loop", #title);
                            break;
                        }
                    }
                }

                debug!("{} cfg file watcher task finished", #title);
            }
        });
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
