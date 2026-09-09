use crate::env::{AppEnv, EnvError, APP_ENV};
use crate::signal::signal_manager_error::SignalManagerError;
use robotech_macros::log_call;
use std::path::PathBuf;
use std::process;
use tokio::sync::broadcast;
use tracing::error;
use wheel_rs::process::{
    check_process, delete_pid_file, get_pid_file_path, read_pid, send_signal_by_instruction,
    watch_signal, PidFileGuard,
};

/// # 信号管理器
///
/// 负责应用进程的信号与 PID 文件管理：
/// - 根据启动指令（`start`/`restart`/`stop`/`kill` 等）向旧进程发送信号或校验运行状态
/// - 创建并持有 PID 文件，防止应用重复启动
/// - 提供系统信号广播接收端，供应用实现优雅退出
#[derive(Debug)]
pub struct SignalManager {
    pid_file_path: PathBuf,
    pid_file_guard: Option<PidFileGuard>,
}

impl SignalManager {
    /// # 创建信号管理器
    ///
    /// 根据 `signal_instruction` 解析启动指令并执行对应操作（如向旧进程发送信号、
    /// 校验程序是否已运行），同时计算并保存 PID 文件路径。
    ///
    /// ## 参数
    /// * `signal_instruction` - 启动指令，如 `start`、`restart`、`stop`、`kill`
    ///
    /// ## 返回值
    /// 返回 `Ok((SignalManager, Option<u32>))`，其中 `Option<u32>` 为旧进程 PID
    /// （不存在时返回 `None`）；操作失败时返回 `Err(SignalManagerError)`。
    #[log_call]
    pub fn new(signal_instruction: String) -> Result<(Self, Option<u32>), SignalManagerError> {
        let AppEnv { app_file_path, .. } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;
        let pid_file_path = get_pid_file_path(app_file_path);
        let old_pid = Self::parse_and_handle_signal_args(signal_instruction, &pid_file_path)?;

        Ok((
            Self {
                pid_file_path,
                pid_file_guard: None,
            },
            old_pid,
        ))
    }

    /// # 注册系统信号监听
    ///
    /// 创建 PID 文件并返回系统信号广播接收端，供应用循环等待退出信号。
    ///
    /// ## 返回值
    /// 返回 `Ok(broadcast::Receiver<Signal>)`；PID 文件创建失败时返回
    /// `Err(SignalManagerError)`。
    pub fn watch_signal(
        &mut self,
    ) -> Result<broadcast::Receiver<nix::sys::signal::Signal>, SignalManagerError> {
        self.pid_file_guard = Some(PidFileGuard::new(self.pid_file_path.clone())?);
        Ok(watch_signal())
    }

    /// # 解析并处理信号参数
    ///
    /// 根据启动指令执行相应操作：
    /// - `restart`：若旧进程在运行，返回其 PID；否则返回 `None`
    /// - `start`：若 PID 文件存在且进程正在运行，返回 `ProgramIsRunning` 错误；
    ///   否则返回 `None`
    /// - 其他指令（如 `stop`/`kill`）：向旧进程发送信号，`kill` 时顺带删除
    ///   PID 文件，操作完成后进程直接退出
    ///
    /// ## 参数
    /// * `signal_instruction` - 启动指令字符串
    /// * `pid_file_path` - PID 文件路径
    ///
    /// ## 返回值
    /// 返回 `Ok(Option<u32>)`，即旧进程 PID（不存在时为 `None`）；
    /// 处理失败时返回 `Err(SignalManagerError)`。
    #[log_call]
    fn parse_and_handle_signal_args(
        signal_instruction: String,
        pid_file_path: &PathBuf,
    ) -> Result<Option<u32>, SignalManagerError> {
        let old_pid = read_pid(pid_file_path)?;
        if signal_instruction == "restart" {
            // 不处理，直接返回(restart指令在本函数中不处理，后续在需要时再单独发送信号停止旧程序)
            if let Some(old_pid) = old_pid
                && check_process(old_pid)?
            {
                return Ok(Some(old_pid));
            }
            Ok(None)
        } else if signal_instruction == "start" {
            // 如果存在PID文件且进程存在，则报错
            if let Some(old_pid) = old_pid
                && check_process(old_pid)?
            {
                Err(SignalManagerError::ProgramIsRunning(old_pid))?
            }
            Ok(None)
        } else {
            let old_pid =
                old_pid.ok_or(SignalManagerError::NotFoundPidFile(pid_file_path.clone()))?;
            if let Err(e) = send_signal_by_instruction(&signal_instruction, old_pid) {
                error!("Failed to send signal: {e}");
                process::exit(1);
            } else {
                if signal_instruction == "kill" {
                    if let Err(e) = delete_pid_file(&pid_file_path) {
                        error!("Failed to delete pid file: {e}");
                        process::exit(1);
                    }
                }
                process::exit(0);
            };
        }
    }
}
