use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Block, Expr, Token};

pub(super) struct WatchFileArgs {
    title: String,
    files: Expr,
    on_files_changed: Block,
}

impl Parse for WatchFileArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let title = input.parse::<syn::LitStr>()?.value();
        let _: Token![,] = input.parse()?;

        let files = input.parse::<Expr>()?;

        let _: Token![,] = input.parse()?;
        let on_files_changed = input.parse()?;

        Ok(WatchFileArgs {
            title,
            files,
            on_files_changed,
        })
    }
}

pub(super) fn watch_file_macro(args: WatchFileArgs) -> TokenStream {
    let WatchFileArgs {
        title,
        files,
        on_files_changed,
    } = args;
    let on_files_changed = &on_files_changed.stmts;

    let expanded = quote! {
        use notify_debouncer_mini::DebouncedEventKind;


        tracing::debug!("watch {} file: {:?} ...", #title, #files);
        tokio::spawn({
            async move {
                let (_watcher, receiver) = watch_file(#files).expect(&format!("watch {} file error: {:?}", #title, #files));

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
                                        tracing::debug!("{} file trigger {:?}: {:?}", #title, event, #files);
                                        if event.kind == DebouncedEventKind::AnyContinuous {
                                            // 事件持续发生，防抖超时了
                                            continue;
                                        }
                                    }
                                    tracing::debug!("{} file changed: {:?} ...", #title, #files);

                                    #( #on_files_changed )*
                                }
                                Err(e) => {
                                    tracing::warn!("error receiving {} file events: {:?} {:?}", #title, #files, e);
                                }
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // 没有消息，继续下一次循环
                            continue;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            // 通道关闭
                            tracing::debug!("{} file watcher channel closed, exiting watcher loop: {:?}", #title, #files);
                            break;
                        }
                    }
                }

                tracing::debug!("{} file watcher task finished: {:?}", #title, #files);
            }
        });
    };

    // 调试：打印完整展开的代码
    // println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
