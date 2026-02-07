use crate::env::{Env, ENV};
use crate::signal::signal_manager_error::SignalManagerError;
use libc::pid_t;
use log::{debug, error};
use std::path::PathBuf;
use std::process;
use wheel_rs::process::{
    check_process, read_pid, send_signal_by_instruction, watch_signal, PidFileGuard,
};

pub struct SignalManager {
    _pid_file_guard: PidFileGuard,
    pub old_pid: Option<pid_t>,
}

impl SignalManager {
    pub fn new(signal_instruction: String) -> Result<Self, SignalManagerError> {
        debug!("初始化信号管理者");
        let Env { app_file_path, .. } = ENV.get().expect("Environment not initialized");
        let old_pid = Self::parse_and_handle_signal_args(signal_instruction, app_file_path)?;
        let pid_file_guard = PidFileGuard::new(app_file_path)?;
        // 监听系统信号
        watch_signal();
        Ok(Self {
            _pid_file_guard: pid_file_guard,
            old_pid,
        })
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
    fn parse_and_handle_signal_args(
        signal_instruction: String,
        app_file_path: &PathBuf,
    ) -> Result<Option<pid_t>, SignalManagerError> {
        debug!("parse_and_handle_signal_args: {:?}", signal_instruction);
        if signal_instruction == "restart" {
            // 不处理，直接返回(restart指令在本函数中不处理，后续在需要时再单独发送信号停止旧程序)
            if let Some(pid) = read_pid(app_file_path)?
                && check_process(pid)?
            {
                return Ok(Some(pid));
            }
            Ok(None)
        } else if signal_instruction == "start" {
            // 如果存在PID文件且进程存在，则报错
            if let Some(pid) = read_pid(app_file_path)?
                && check_process(pid)?
            {
                Err(SignalManagerError::ProgramIsRunning(app_file_path.clone()))?
            }
            Ok(None)
        } else {
            let pid = read_pid(app_file_path)?
                .ok_or(SignalManagerError::NotFoundPidFile(app_file_path.clone()))?;
            if let Err(e) = send_signal_by_instruction(&signal_instruction, pid) {
                error!("Failed to send signal: {e}");
                process::exit(1);
            } else {
                process::exit(0);
            };
        }
    }
}
