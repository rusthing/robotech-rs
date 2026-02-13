use crate::env::{AppEnv, EnvError, APP_ENV};
use crate::signal::signal_manager_error::SignalManagerError;
use libc::pid_t;
use log::{debug, error};
use std::path::PathBuf;
use std::process;
use std::sync::RwLock;
use tokio::sync::oneshot;
use tracing::instrument;
use wheel_rs::process::{
    check_process, delete_pid_file, get_pid_file_path, read_pid, send_signal_by_instruction,
    watch_signal, PidFileGuard,
};

static PID_FILE_GUARD: RwLock<Option<PidFileGuard>> = RwLock::new(None);

#[derive(Debug)]
pub struct SignalManager;
impl Drop for SignalManager {
    fn drop(&mut self) {
        let mut pid_file_guard_lock = PID_FILE_GUARD
            .write()
            .expect("Failed to write to PID_FILE_GUARD");
        *pid_file_guard_lock = None;
    }
}

impl SignalManager {
    #[instrument(level = "debug", ret, err)]
    pub fn new(
        signal_instruction: String,
    ) -> Result<(Self, Option<pid_t>, oneshot::Sender<()>), SignalManagerError> {
        debug!("初始化信号管理者...");
        let AppEnv { app_file_path, .. } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;
        let pid_file_path = get_pid_file_path(app_file_path);
        let old_pid = Self::parse_and_handle_signal_args(signal_instruction, &pid_file_path)?;

        let (app_started_sender, app_stated_receiver) = oneshot::channel::<()>();

        // 监听系统信号
        watch_signal();

        tokio::spawn(async move {
            if let Ok(_) = app_stated_receiver.await
                && let Ok(pid_file_guard) = PidFileGuard::new(pid_file_path)
            {
                let mut pid_file_guard_lock = PID_FILE_GUARD
                    .write()
                    .expect("Failed to write to PID_FILE_GUARD");
                *pid_file_guard_lock = Some(pid_file_guard);
            }
        });

        Ok((Self {}, old_pid, app_started_sender))
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
    #[instrument(level = "debug", ret, err)]
    fn parse_and_handle_signal_args(
        signal_instruction: String,
        pid_file_path: &PathBuf,
    ) -> Result<Option<pid_t>, SignalManagerError> {
        debug!("解析并处理信号参数...");
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
