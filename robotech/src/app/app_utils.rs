use crate::app::AppError;
use crate::cfg::{build_cfg, deserialize_config, BaseConfig};
use crate::env::{AppEnv, EnvError, APP_ENV};
use crate::log::LogConfig;
#[cfg(feature = "config-center")]
use crate::micro_svc::get_hub_client;
use arc_swap::ArcSwap;
use config::{Config, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tracing::{debug, error, info, warn};
use wheel_rs::config_utils::diff_config;
use wheel_rs::file_utils::{watch_file_changed, FileWatcher};
use wheel_rs::process::{get_current_pid, send_signal_by_instruction};

pub type Result<T> = core::result::Result<T, AppError>;

pub struct AppWatcher<T>
where
    T: Clone + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    _cfg_file_watcher: FileWatcher,
    _app_file_watcher: FileWatcher,
    pub app_config: Arc<T>,
    watch_join_handle: tokio::task::JoinHandle<()>,
}

impl<T> Drop for AppWatcher<T>
where
    T: Clone + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    fn drop(&mut self) {
        self.watch_join_handle.abort();
    }
}

impl<T> AppWatcher<T>
where
    T: Clone + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    pub async fn new<F, Fut>(
        config_file_path: Option<String>,
        log_config_changed_tx: watch::Sender<(LogConfig, HashMap<String, Value>)>,
        mut on_change: F,
    ) -> Result<Self>
    where
        F: FnMut(Arc<T>, HashMap<String, Value>) -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let AppEnv {
            app_dir,
            app_file_path,
            app_file_name_without_ext,
            ..
        } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;
        let (base_config, config, files) =
            build_app_cfg(config_file_path.clone(), app_dir, app_file_name_without_ext).await?;
        if let Some(log_config) = base_config.clone().log {
            let changed = HashMap::new();
            log_config_changed_tx.send((log_config, changed))?;
        }
        let app_config: T = deserialize_config::<T>(config.clone()).await?;

        let last_config = Arc::new(ArcSwap::from_pointee(config.clone()));
        let (config_changed_tx, mut config_changed_rx) =
            watch::channel((app_config.clone(), HashMap::new()));
        // let app_config_clone = app_config.clone();
        let watch_join_handle = tokio::spawn(async move {
            info!("watching app config");
            loop {
                match config_changed_rx.changed().await {
                    Ok(_) => {
                        let (app_config, changed) = config_changed_rx.borrow().clone();
                        if let Err(e) = on_change(Arc::new(app_config), changed).await {
                            error!("handle config change error: {e:?}");
                            break;
                        }
                    }
                    Err(err) => {
                        info!("watch config error: {:?}", err);
                        break;
                    }
                }
            }
            info!("exit watch app config");
        });

        let config_changed_tx_clone = config_changed_tx.clone();
        let config_file_path_clone = config_file_path.clone();
        let _cfg_file_watcher =
            watch_file_changed(files.clone(), base_config.watch_debounce_delay, move |_| {
                let config_changed_tx_clone = config_changed_tx_clone.clone();
                let config_file_path = config_file_path_clone.clone();
                let last = Arc::clone(&last_config);
                let old_config = last.load_full();
                async move {
                    match build_app_cfg(config_file_path, app_dir, app_file_name_without_ext).await
                    {
                        Ok((_, new_config, _)) => {
                            let changed = diff_config(&old_config, &new_config);
                            if !changed.is_empty() {
                                info!("log config changed: {:?}", changed);
                                last.store(Arc::new(new_config.clone()));
                                match deserialize_config::<T>(new_config).await {
                                    Ok(app_config) => {
                                        config_changed_tx_clone.send((app_config, changed))?;
                                    }
                                    Err(e) => {
                                        error!("deserialize app config error: {:?}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("build app config error: {:?}", e);
                        }
                    }
                    Ok(())
                }
            })?;

        // 监听应用程序的文件变化，当文件更新时优雅退出应用程序
        let _app_file_watcher =
            watch_app_file(&app_file_path.clone(), base_config.watch_debounce_delay)?;

        #[cfg(feature = "config-center")]
        get_hub_client()?
            .watch_config_changed(move || async move {
                info!("watching config center config");
                Ok(())
            })
            .await?;

        Ok(Self {
            app_config: Arc::new(app_config.clone()),
            watch_join_handle,
            _cfg_file_watcher,
            _app_file_watcher,
        })
    }
}

async fn build_app_cfg(
    config_file_path: Option<String>,
    app_dir: &PathBuf,
    app_file_name_without_ext: &str,
) -> crate::cfg::Result<(BaseConfig, Config, Vec<String>)> {
    build_cfg(
        app_dir,
        "APP",
        Some(app_file_name_without_ext),
        app_file_name_without_ext,
        config_file_path,
    )
    .await
}

/// 监控应用程序的文件变化，当文件更新时优雅退出应用程序
pub fn watch_app_file(app_file_path: &PathBuf, debounce_delay: Duration) -> Result<FileWatcher> {
    let files = vec![app_file_path.to_string_lossy().to_string()];
    Ok(watch_file_changed(files, debounce_delay, |_| async {
        info!("应用程序的文件已更新，优雅退出");
        quit();
        Ok(())
    })?)
}

pub async fn wait_app_exit<F, Fut>(
    mut signal_receiver: broadcast::Receiver<nix::sys::signal::Signal>,
    graceful_shutdown: F,
) -> Result<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    loop {
        match signal_receiver.recv().await {
            Ok(signal) => {
                debug!("收到信号: {:?}", signal);
                match signal {
                    nix::sys::signal::Signal::SIGINT
                    | nix::sys::signal::Signal::SIGTERM
                    | nix::sys::signal::Signal::SIGQUIT => {
                        break;
                    }
                    _ => {}
                }
            }
            Err(err) => {
                warn!("无法接收信号: {}", err);
                break;
            }
        }
    }
    debug!("正在优雅退出...");
    graceful_shutdown().await?;
    debug!("优雅退出完成.");
    Ok(())
}

/// 优雅退出
fn quit() {
    let _ = send_signal_by_instruction("quit", get_current_pid());
}
