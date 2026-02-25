use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::Token;

pub(crate) struct StartWebServerArgs {
    web_server_config: Ident,
    configure: Ident,
    port_of_args: Ident,
    old_pid: Ident,
    app_stated_sender: Ident,
}

impl Parse for StartWebServerArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let web_server_config = input.parse()?;
        let _: Token![,] = input.parse()?;
        let configure = input.parse()?;
        let _: Token![,] = input.parse()?;
        let port_of_args = input.parse()?;
        let _: Token![,] = input.parse()?;
        let old_pid = input.parse()?;
        let _: Token![,] = input.parse()?;
        let app_stated_sender = input.parse()?;

        Ok(StartWebServerArgs {
            web_server_config,
            configure,
            port_of_args,
            old_pid,
            app_stated_sender,
        })
    }
}

pub(crate) fn start_web_server_macro(input: StartWebServerArgs) -> TokenStream {
    let StartWebServerArgs {
        web_server_config,
        configure,
        port_of_args,
        old_pid,
        app_stated_sender,
    } = input;

    let expanded = quote! {
        use log::{debug, error};
        use actix_web::middleware::Logger;
        use actix_web::{App, HttpServer, web};
        use robotech::web::{health, build_cors, create_reusable_listener, terminate_old_web_server, wait_for_web_server_ready, WebServerConfig, WebServerError};

        debug!("初始化Web服务器...");
        let WebServerConfig {
            bind: binds,
            port: mut port_option,
            listen: listens,
            mut reuse_port,
            https: https_config,
            cors: cors_config,
            support_health_check,
            start_wait_timeout,
            start_retry_interval,
            terminate_old_wait_timeout,
            terminate_old_retry_interval,
        } = #web_server_config;

        // 如果命令行参数指定了端口，则使用命令行指定的端口
        if #port_of_args.is_some() {
            port_option = #port_of_args;
        }

        // 是否随机端口
        let mut is_random_port = true;
        let port = port_option.unwrap_or(0);
        if port != 0 {
            is_random_port = false;
        }

        let mut listen_binds = vec![];
        // 解析绑定地址
        if !binds.is_empty() {
            for bind in binds {
                listen_binds.push((bind, port));
            }
        } else if listens.is_empty() {
            // 如果bind和listen都未配置，默认绑定 "0.0.0.0"
            listen_binds.push(("0.0.0.0".to_string(), port));
        }

        // 解析监听地址
        for listen in &listens {
            // 解析地址，从右侧开始分割，最多产生2部分，可以支持IPv4和IPv6，parts[0]为端口，parts[1]为IP地址
            let parts: Vec<&str> = listen.rsplitn(2, ':').collect();
            match parts.len() {
                1 => {
                    let port: u16 = listen
                        .parse()
                        .map_err(|_| WebServerError::ParsePort(listen.to_string()))?;
                    if port != 0 {
                        is_random_port = false;
                    }
                    listen_binds.push(("::".to_string(), port));
                }
                2 => {
                    let port: u16 = parts[0]
                        .parse()
                        .map_err(|_| WebServerError::ParsePort(listen.to_string()))?;
                    if port != 0 {
                        is_random_port = false;
                    }
                    let mut bind = parts[1].to_string();
                    // 如果是IPv6地址，去除方括号
                    if bind.starts_with('[') && bind.ends_with(']') {
                        bind = bind[1..bind.len() - 1].to_string();
                    }
                    listen_binds.push((bind, port));
                }
                _ => Err(WebServerError::ParsePort(listen.to_string()))?,
            }
        }

        // 如果是随机端口，端口复用无意义
        if is_random_port {
            reuse_port = false;
        }

        // 如果不是随机端口，且不是复用端口，且是重启服务器，则先停止旧服务器，再启动新服务器
        if !is_random_port && !reuse_port {
            terminate_old_web_server(
                #old_pid,
                terminate_old_wait_timeout,
                terminate_old_retry_interval,
            )
            .await?;
        }

        // 是否支持健康检查
        let support_health_check = is_random_port || reuse_port || support_health_check;

        let mut http_server = HttpServer::new(move || {
            debug!("HttpServer创建worker，并拥有独立的app...");
            let mut app = App::new()
                .wrap(Logger::default())
                .wrap(build_cors(&cors_config))
                .configure(#configure);

            if support_health_check {
                debug!("支持健康检查");
                app = app.service(health);
            }

            debug!("HttpServer创建worker，并配置完成app.");
            app
        });

        debug!("监听绑定地址...");
        for (bind, port) in &listen_binds {
            if reuse_port {
                debug!("支持端口复用");
                let tcp_listener = create_reusable_listener(bind, *port)?;
                http_server = http_server
                    .listen(tcp_listener)
                    .map_err(|e| WebServerError::Socket(format!("监听自定义tcp socket失败: {}", e)))?;
            } else {
                let ip = bind.to_string();
                http_server = http_server.bind((ip.to_string(), *port)).map_err(|e| {
                    WebServerError::Socket(format!("绑定地址失败: {}:{} - {}", ip, port, e).to_string())
                })?;
            }
        }

        let server = http_server.run();
        tokio::spawn(async move {
            let protocol = if let Some(https_config) = https_config
                && https_config.enabled
            {
                "https"
            } else {
                "http"
            };
            let (ip, port) = &listen_binds[0];
            let ip = if ip == "0.0.0.0" {
                "127.0.0.1"
            } else if ip == "::" {
                "[::1]"
            } else {
                &ip
            };
            let health_url = format!("{}://{}:{}/health", protocol, ip, port);

            if let Err(e) = wait_for_web_server_ready(
                health_url.as_str(),
                start_wait_timeout,
                start_retry_interval,
            )
            .await
            {
                error!("启动Web服务器超时: {}", e);
                return;
            }

            if let Err(_) = #app_stated_sender.send(()) {
                error!("发送应用启动完成消息错误");
                return;
            };

            // 如果是随机端口或复用端口，则可以在前面先启动新服务器，后面这里再停止旧服务器
            if is_random_port || reuse_port {
                if let Err(e) = terminate_old_web_server(
                    #old_pid,
                    terminate_old_wait_timeout,
                    terminate_old_retry_interval,
                )
                .await
                {
                    error!("停止旧Web服务器超时: {}", e);
                    return;
                }
            }
        });

        debug!("启动Web服务器...");
        server.await?;
    };

    // 调试：打印完整展开的代码
    println!("Full expanded code:\n{expanded}");

    TokenStream::from(expanded)
}
