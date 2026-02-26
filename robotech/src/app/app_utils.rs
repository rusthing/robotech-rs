use crate::cfg::{build_cfg, CfgError};
use log::{debug, warn};
use robotech_macros::log_call;
use tokio::sync::broadcast;

#[log_call]
pub fn build_app_cfg<'a, T: serde::Deserialize<'a> + std::fmt::Debug>(
    path: Option<String>,
) -> Result<(T, Vec<String>), CfgError> {
    build_cfg("APP", None, path)
}

pub async fn wait_app_exit(mut signal_receiver: broadcast::Receiver<nix::sys::signal::Signal>) {
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
}
