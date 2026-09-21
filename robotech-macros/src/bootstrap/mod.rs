//! # 应用启动引导过程宏
//!
//! 实现 `bootstrap!()` 过程宏，自动生成微服务应用的 `main()` 函数与
//! CLI 参数定义，消除各项目 `main.rs` 中的重复模板代码。

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::TypePath;

/// 宏参数：接收一个类型路径，即 AppConfig 的完整路径
pub(super) struct BootstrapArgs {
    app_config_type: TypePath,
}

impl Parse for BootstrapArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let app_config_type = input.parse()?;
        Ok(BootstrapArgs { app_config_type })
    }
}

/// 生成微服务应用的 `main()` 函数和 CLI 参数定义
///
/// ## 使用示例
///
/// ```ignore
/// use robotech::bootstrap;
/// use msg_svr::config::AppConfig;
///
/// bootstrap!(AppConfig);
///
/// #[log_call]
/// async fn setup(
///     app_config: &Arc<AppConfig>,
///     changed: &Option<HashMap<String, Value>>,
///     port: Option<u16>,
///     old_pid: Option<u32>,
/// ) -> Result<(), anyhow::Error> {
///     // 项目特有的初始化逻辑
///     Ok(())
/// }
/// ```
pub fn bootstrap_macro(args: BootstrapArgs) -> TokenStream {
    let app_config_type = args.app_config_type;

    let expanded = quote! {
        use clap::Parser;
        use robotech::app::{wait_app_exit, AppWatcher};
        use robotech::dao::init_dao;
        use robotech::env::init_env;
        use robotech::log::LogWatcher;
        use robotech::micro_svc::{drop_hub_client, register_micro_svc};
        use robotech::signal::SignalManager;
        use robotech::web::stop_web_service;
        use tracing::info;

        #[derive(Parser, Debug, Clone)]
        #[command(
            author = env!("CARGO_PKG_AUTHORS"),
            version,
            about,
            help_template = "{name} v{version} - {about}\n\nAUTHOR: {author}\n\nUSAGE: {usage}\n\nOPTIONS:\n{options}"
        )]
        struct Args {
            /// 配置文件的路径
            #[arg(short, long)]
            config_file: Option<String>,

            /// Web服务器的端口号
            #[arg(short, long)]
            port: Option<u16>,

            /// 监听信号
            #[arg(
                short,
                long,
                default_value = "start",
                long_help = r#"监听信号，支持指令如下:
    start - 默认值，先发送 SIGCONT 信号(kill -0)，检查程序是否已运行，然后启动程序
    restart - 先发送 SIGTERM 信号(kill -15)，如果旧程序已运行，收到信号后会停止运行，然后启动新程序
    stop/s - 发送 SIGTERM 信号(kill -15)，用于终止程序，优雅退出
    kill/k - 发送 SIGKILL 信号(kill -9)，用于强制终止程序"#
            )]
            signal: String,
        }

        #[tokio::main]
        async fn main() -> anyhow::Result<()> {
            let Args {
                signal,
                config_file: config_file_path,
                port,
            } = Args::parse();

            init_env()?;
            let log_watcher = LogWatcher::new().await?;
            init_dao()?;

            let (mut signal_manager, old_pid) = SignalManager::new(signal)?;

            let app_watcher: AppWatcher<#app_config_type> = AppWatcher::new(
                config_file_path,
                log_watcher.config_changed_tx.clone(),
                move |app_config: Arc<#app_config_type>, changed| async move {
                    let changed = Some(changed);
                    setup(&app_config, &changed, port, old_pid).await?;
                    info!("重新加载配置成功");
                    Ok(())
                },
            )
            .await?;

            let changed = None;
            setup(&app_watcher.app_config, &changed, port, old_pid).await?;

            register_micro_svc().await;

            let signal_receiver = signal_manager.watch_signal()?;
            Ok(wait_app_exit(signal_receiver, || async move {
                drop_hub_client().await;
                stop_web_service().await.expect("无法停止旧的Web服务");
                Ok(())
            })
            .await?)
        }
    };

    TokenStream::from(expanded)
}