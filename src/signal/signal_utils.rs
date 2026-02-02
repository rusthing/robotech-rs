use crate::signal::pid_utils::{delete_pid, read_pid, write_pid, PidFileGuard};
use log::{error, info};
use nix::sys::signal::kill;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use std::process;

/// # 解析并处理信号参数
///
/// 该函数根据传入的信号参数执行相应操作，如发送系统信号给指定进程或启动程序。
///
/// ## 参数
///
/// * `signal` - 信号参数，可选字符串，表示要执行的信号操作
///
/// ## 返回值
///
/// 返回 `PidFileGuard` 实例，用于管理PID文件的生命周期
///
/// ## 支持的信号指令
///
/// * `start` - 不发送信号，直接启动程序
/// * `restart` - 发送 SIGINT 信号并立即返回
/// * `reload`/`l` - 发送 SIGHUP 信号，用于重载配置
/// * `quit`/`q` - 发送 SIGINT 信号，用于优雅退出
/// * `stop`/`s` - 发送 SIGTERM 信号，用于终止程序
/// * `kill`/`k` - 发送 SIGKILL 信号，用于强制终止程序
/// 
/// ## 使用示例
///
/// ```
/// let guard = parse_and_handle_signal_args(Some("stop".to_string()));
/// ```
///
/// # Panics
///
/// 当PID文件已存在且对应进程正在运行时，函数会panic并输出提示信息
pub fn parse_and_handle_signal_args(signal: Option<String>) -> PidFileGuard {
    info!("Signal signal: {:?}", signal);
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

/// # 发送系统信号给指定进程
///
/// 根据信号字符串向目标进程发送相应的系统信号，支持多种常用信号。
///
/// ## 参数
///
/// * `signal_str` - 信号名称字符串，如 "stop", "reload", "quit", "kill" 等
/// * `pid_option` - 进程ID选项，指定要发送信号的目标进程
///
/// ## 返回值
///
/// * `Ok(())` - 信号发送成功
/// * `Err(std::io::Error)` - 信号发送失败
///
/// ## 支持的信号指令
///
/// * `start` - 不发送信号，直接启动程序
/// * `restart` - 发送 SIGINT 信号并立即返回
/// * `reload`/`l` - 发送 SIGHUP 信号，用于重载配置
/// * `quit`/`q` - 发送 SIGINT 信号，用于优雅退出
/// * `stop`/`s` - 发送 SIGTERM 信号，用于终止程序
/// * `kill`/`k` - 发送 SIGKILL 信号，用于强制终止程序
///
/// ## Panics
///
/// 当指定的信号名称无效时，函数会panic并输出错误信息
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
        "kill" | "k" => {
            kill(Pid::from_raw(pid), Signal::SIGKILL).expect("SIGKILL");
            delete_pid();
        }
        _ => panic!("Invalid signal({signal_str})"),
    };
    process::exit(0);
}

/// # 异步等待系统信号
///
/// 该函数异步等待系统信号的到来，目前为空实现，可用于扩展信号处理功能。
///
/// ## 使用示例
///
/// ```
/// # use tokio;
/// # #[tokio::main]
/// # async fn main() {
/// wait_for_signal().await;
/// # }
/// ```
pub async fn wait_for_signal() {}
