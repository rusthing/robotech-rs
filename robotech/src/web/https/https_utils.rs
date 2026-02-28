use crate::web::{HttpsConfig, WebServerError};
use axum::Router;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server;
use log::{debug, error};
use rustls_pemfile::{certs, private_key};
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, OnceLock};
use tokio::net::TcpListener;
use tokio::sync::broadcast::Receiver;
use tokio::task::JoinHandle;
use tokio_rustls::rustls::crypto::aws_lc_rs;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

static CRYPTO_PROVIDER_INITIALIZED: OnceLock<()> = OnceLock::new();

pub fn build_https(
    router: Router,
    tokio_listener: TcpListener,
    mut web_server_stop_receiver: Receiver<()>,
    https_config: HttpsConfig,
) -> Result<JoinHandle<()>, WebServerError> {
    let HttpsConfig { cert, key, .. } = https_config;

    CRYPTO_PROVIDER_INITIALIZED.get_or_init(|| {
        aws_lc_rs::default_provider()
            .install_default()
            .expect("Failed to install rustls crypto provider");
    });

    // 加载证书和私钥
    let cert_file = &mut BufReader::new(
        File::open(cert.ok_or_else(|| WebServerError::Config("未配置https的cert".to_string()))?)
            .map_err(|e| WebServerError::Config(format!("不能打开cert文件-{}", e.to_string())))?,
    );
    let key_file = &mut BufReader::new(
        File::open(key.ok_or_else(|| WebServerError::Config("未配置https的key".to_string()))?)
            .map_err(|e| WebServerError::Config(format!("不能打开key文件-{}", e.to_string())))?,
    );
    // 加载证书
    let cert_chain = certs(cert_file)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| WebServerError::ParseHttpsCert(format!("读取证书链失败: {}", e)))?;

    if cert_chain.is_empty() {
        return Err(WebServerError::ParseHttpsCert(
            "证书文件中未找到有效证书".to_string(),
        ));
    }

    let key = private_key(key_file)
        .map_err(|e| WebServerError::ParseHttpsKey(e.to_string()))?
        .ok_or_else(|| WebServerError::ParseHttpsKey("No private key found".to_string()))?;
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .map_err(|e| WebServerError::ParseHttpsCert(format!("TLS配置失败: {}", e)))?;

    // 配置 ALPN，支持 HTTP/2 和 HTTP/1.1
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    let tls_acceptor = TlsAcceptor::from(Arc::new(config));
    let router = router.clone();
    let handle = tokio::spawn(async move {
        loop {
            let router = router.clone();
            // 等待新的客户端连接
            let (tcp_stream, client_socket_addr) = tokio::select! {
                result = tokio_listener.accept() => {
                    match result {
                        Ok(accepted) => accepted,
                        Err(e) => {
                            error!("Accept error: {}", e);
                            continue;
                        }
                    }
                }
                _ = web_server_stop_receiver.recv() => {
                    debug!("Stopping accept loop.");
                    break;
                }
            };

            let tls_acceptor = tls_acceptor.clone();
            let mut web_server_stop_receiver = web_server_stop_receiver.resubscribe();
            tokio::spawn(async move {
                // TLS 握手
                match tls_acceptor.accept(tcp_stream).await {
                    Ok(tls_stream) => {
                        // 将 TlsStream 包装为 Hyper 认识的 TokioIo
                        let io = TokioIo::new(tls_stream);
                        use tower::ServiceExt;
                        // 使用 Hyper 的 Builder 服务单个连接
                        let hyper_service = service_fn(move |request| {
                            let router = router.clone();
                            async move { router.oneshot(request).await }
                        });
                        let builder = server::conn::auto::Builder::new(TokioExecutor::new());
                        let conn = builder.serve_connection_with_upgrades(io, hyper_service);
                        let mut conn = std::pin::pin!(conn);
                        tokio::select! {
                            result = conn.as_mut() => {
                                if let Err(e) = result {
                                    error!("Connection error: {}", e);
                                }
                            }
                            _ = web_server_stop_receiver.recv() => {
                                conn.as_mut().graceful_shutdown();
                                // 等连接真正关闭
                                if let Err(e) = conn.as_mut().await {
                                    error!("Connection error during shutdown from {}: {}", client_socket_addr, e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("TLS握手失败: {}", e);
                    }
                }
            });
        }
    });
    Ok(handle)
}
