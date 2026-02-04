use crate::main_fn::pid_utils::{delete_pid_file, read_pid};
use crate::main_fn::{write_pid, PidFileGuard};
use libc::pid_t;
use log::{debug, error, info};
use nix::sys::signal::kill;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use std::process;
use tokio::signal::unix::{signal, SignalKind};

pub fn init_signal(signal: String) -> (PidFileGuard, Option<pid_t>) {
    debug!("初始化信号");
    let old_pid = parse_and_handle_signal_args(signal);
    let pid_file_guard = write_pid();
    // 监听系统信号
    watch_signal();
    (pid_file_guard, old_pid)
}
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
/// * `start` - 默认值，先发送`SIGCONT`信号(kill -0)，检查程序是否已运行(如果程序已运行，会报错)，然后启动程序
/// * `restart` - 不处理，直接返回(restart指令在本函数中不处理，后续在需要时再单独发送信号停止旧程序)
/// * `stop`/`s` - 发送`SIGTERM`信号(kill -15)，用于终止程序，优雅退出
/// * `kill`/`k` - 发送`SIGKILL`信号(kill -9)，用于强制终止程序(顺带删除PID文件)
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
fn parse_and_handle_signal_args(signal: String) -> Option<pid_t> {
    debug!("parse_and_handle_signal_args: {:?}", signal);
    if signal == "restart" {
        // 不处理，直接返回(restart指令在本函数中不处理，后续在需要时再单独发送信号停止旧程序)
        if let Some(pid) = read_pid()
            && send_signal_to_check(pid)
        {
            return Some(pid);
        }
        None
    } else if signal == "start" {
        // 如果存在PID文件且进程存在，则报错
        if let Some(pid) = read_pid()
            && send_signal_to_check(pid)
        {
            panic!(
                "PID file already exists, please confirm whether the program is running. If not, please delete the PID file before running the program again."
            );
        }
        None
    } else {
        let pid = read_pid().expect("Failed to read pid");
        if let Err(e) = send_signal(&signal, pid) {
            error!("Failed to send signal: {e}");
            process::exit(1);
        } else {
            process::exit(0);
        };
    }
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
/// * `stop`/`s` - 发送`SIGTERM`信号 (kill -15)，用于终止程序，优雅退出
/// * `kill`/`k` - 发送`SIGKILL`信号(kill -9)，用于强制终止程序(顺带删除PID文件)
///
/// ## Panics
///
/// 当指定的信号名称无效时，函数会panic并输出错误信息
fn send_signal(signal_str: &str, pid: i32) -> std::io::Result<()> {
    debug!("send signal: {signal_str} -> {pid}");
    let signal_str = signal_str.to_lowercase();
    Ok(match signal_str.as_str() {
        "stop" | "s" => kill(Pid::from_raw(pid), Signal::SIGTERM).expect("SIGTERM"),
        "kill" | "k" => {
            kill(Pid::from_raw(pid), Signal::SIGKILL).expect("SIGKILL");
            delete_pid_file();
        }
        _ => panic!("Invalid signal({signal_str})"),
    })
}

pub(crate) async fn send_signal_to_stop(old_pid: i32) -> Result<(), String> {
    send_signal("stop", old_pid).expect("Failed to send signal: ");
    wait_for_process_exit(old_pid).await?;
    Ok(())
}

const MAX_RETRIES: u32 = 20; // 最大重试次数
const RETRY_INTERVAL_MS: u64 = 500; // 重试间隔时间（毫秒）
async fn wait_for_process_exit(pid: i32) -> Result<(), String> {
    tokio::spawn(async move {
        let mut retries = 0;
        while send_signal_to_check(pid) {
            // 进程仍然存在，继续等待
            retries += 1;
            if retries >= MAX_RETRIES {
                return Err(format!(
                    "Process is not exit after sending stop signal. PID: {pid}"
                ));
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_INTERVAL_MS)).await;
        }
        Ok(())
    })
    .await
    .expect(&format!("Failed to wait for process exit: {}", pid))
}

pub(crate) fn send_signal_to_check(pid: i32) -> bool {
    kill(Pid::from_raw(pid), Signal::SIGCONT).is_ok()
}

/// # 异步监听系统信号
///
/// 该函数异步等待系统信号的到来，目前为空实现，可用于扩展信号处理功能。
pub fn watch_signal() {
    tokio::spawn(async move {
        debug!("watching signal...");
        let mut sigint_stream =
            signal(SignalKind::interrupt()).expect("Failed to register signal handler: SIGINT");
        let mut sigterm_stream =
            signal(SignalKind::terminate()).expect("Failed to register signal handler: SIGTERM");

        loop {
            tokio::select! {
                _ = sigint_stream.recv() => {
                    info!("程序中断运行(SIGINT)");
                    break;
                }
                _ = sigterm_stream.recv() => {
                    info!("程序终止运行(SIGTERM)");
                    break;
                }
            }
        }
    });
}
