use crate::app::AppError;
use crate::cfg::build_cfg;
use crate::env::{AppEnv, EnvError, APP_ENV};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use robotech_macros::log_call;
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, warn};

#[log_call]
pub fn build_app_cfg<'a, T: serde::Deserialize<'a> + std::fmt::Debug>(
    path: Option<String>,
) -> Result<(T, Vec<String>), AppError> {
    Ok(build_cfg("APP", None, path)?)
}

pub fn add_app_file_to_watch(files: &mut Vec<String>) -> Result<(), EnvError> {
    let AppEnv { app_file_path, .. } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;
    files.push(app_file_path.to_string_lossy().to_string());
    Ok(())
}

pub fn watch_file(
    files: Arc<Vec<String>>,
) -> Result<
    (
        Debouncer<RecommendedWatcher>,
        mpsc::Receiver<DebounceEventResult>,
    ),
    notify::Error,
> {
    let (sender, receiver) = mpsc::channel();

    let mut debouncer = new_debouncer(
        Duration::from_millis(500), // 防抖延迟时间
        sender,
    )?;

    let watcher = debouncer.watcher();

    // 开始监控
    for file in &*files {
        watcher.watch(Path::new(&file), RecursiveMode::NonRecursive)?;
    }

    Ok((debouncer, receiver))
}

pub async fn wait_app_exit<F, Fut>(
    mut signal_receiver: broadcast::Receiver<nix::sys::signal::Signal>,
    graceful_shutdown: F,
) -> Result<(), AppError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<(), AppError>>,
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
