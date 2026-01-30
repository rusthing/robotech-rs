use crate::args::pid_utils::{read_pid, write_pid};
use log::{error, info};
use nix::sys::signal;
use nix::unistd::Pid;
use std::process;
use tokio::signal::unix::{signal, Signal, SignalKind};

pub fn parse_and_handle_signal_args(signal: Option<String>) {
    info!("Signal args: {:?}", signal);
    if let Some(signal) = signal {
        let _ = send_signal(&signal).map_err(|e| {
            error!("Failed to send signal: {}", e);
            process::exit(1);
        });
    }
    write_pid();
}

fn send_signal(signal_str: &str) -> std::io::Result<Signal> {
    let pid = read_pid().expect("Failed to read pid");
    let signal_str = signal_str.to_lowercase();
    match signal_str.as_str() {
        "reload" | "r" => signal::kill(Pid::from_raw(pid), signal::Signal::SIGHUP).expect("SIGHUP"),
        "stop" | "s" => signal::kill(Pid::from_raw(pid), signal::Signal::SIGTERM).expect("SIGTERM"),
        _ => panic!("Invalid signal({signal_str})"),
    };
    process::exit(0);
}

pub async fn wait_for_signal() {}
