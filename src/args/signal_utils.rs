use crate::args::pid_utils::{read_pid, write_pid, PidFileGuard};
use log::{error, info};
use nix::sys::signal::kill;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use std::process;

pub fn parse_and_handle_signal_args(signal: Option<String>) -> PidFileGuard {
    info!("Signal args: {:?}", signal);
    let pid_option = read_pid();
    if let Some(signal) = signal
        && signal != "start"
    {
        let _ = send_signal(&signal, &pid_option).map_err(|e| {
            error!("Failed to send signal: {}", e);
            process::exit(1);
        });
    } else {
        // 没有信号参数或者信号参数为start，如果存在PID文件且进程存在(发送信号 0 来检查进程是否存在)，则提示并退出
        if let Some(pid) = pid_option
            && kill(Pid::from_raw(pid), Signal::SIGCONT).is_ok()
        {
            panic!(
                "PID file already exists, please confirm whether the program is running. If not, please delete the PID file before running the program again."
            );
        };
    }
    write_pid()
}

fn send_signal(signal_str: &str, pid_option: &Option<i32>) -> std::io::Result<()> {
    let pid = pid_option.expect("Failed to read pid");
    let signal_str = signal_str.to_lowercase();
    match signal_str.as_str() {
        "restart" => {
            let _ = kill(Pid::from_raw(pid), Signal::SIGINT);
            return Ok(());
        }
        "load" | "reload" | "l" => kill(Pid::from_raw(pid), Signal::SIGHUP).expect("SIGHUP"),
        "quit" | "q" => kill(Pid::from_raw(pid), Signal::SIGINT).expect("SIGINT"),
        "stop" | "s" => kill(Pid::from_raw(pid), Signal::SIGTERM).expect("SIGTERM"),
        "kill" | "k" => kill(Pid::from_raw(pid), Signal::SIGKILL).expect("SIGKILL"),
        _ => panic!("Invalid signal({signal_str})"),
    };
    process::exit(0);
}

pub async fn wait_for_signal() {}
