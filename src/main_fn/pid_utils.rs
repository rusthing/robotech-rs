use crate::env::{Env, ENV};
use libc::pid_t;
use log::{debug, info};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process;

/// # PID 文件守卫结构
///
/// 用于管理 PID 文件的生命周期，通过 RAII 模式在程序退出时自动删除 PID 文件。
///
/// ## 使用示例
///
/// ```
/// let guard = write_pid();  // 创建 PID 文件并返回守卫
/// // 当 guard 超出作用域时，会自动删除 PID 文件
/// ```
pub struct PidFileGuard;

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        delete_pid_file_if_my_process();
    }
}

fn get_pid_file_path() -> PathBuf {
    let Env { app_file_path, .. } = ENV.get().expect("ENV is None");
    let mut pid_file_path = app_file_path.clone();
    pid_file_path.add_extension("pid");
    pid_file_path
}

/// # 从PID文件中读取进程ID
///
/// 尝试从应用程序对应的PID文件中读取进程ID。如果文件不存在或读取失败，则返回None。
///
/// ## Returns
///
/// 返回 `Option<pid_t>`，如果成功读取则包含进程ID，否则为None
///
/// ## Examples
///
/// ```
/// match read_pid() {
///     Some(pid) => println!("当前进程ID: {}", pid),
///     None => println!("无法读取PID文件或文件不存在"),
/// }
/// ```
pub(crate) fn read_pid() -> Option<pid_t> {
    debug!("Reading PID...");
    let pid_file_path = get_pid_file_path();
    if !pid_file_path.exists() {
        return None;
    }
    let pid_file = pid_file_path.to_str().unwrap();
    let pid_file = File::open(pid_file).unwrap();
    let reader = BufReader::new(pid_file);
    let mut lines = reader.lines();
    Some(
        lines
            .next()
            .unwrap()
            .unwrap()
            .trim()
            .parse::<pid_t>()
            .unwrap(),
    )
}

/// # 将当前进程ID写入PID文件
///
/// 创建一个PID文件并将当前进程的ID写入其中，同时返回一个PidFileGuard来管理文件的生命周期。
/// 当PidFileGuard超出作用域时，会自动清理PID文件。
///
/// ## Returns
///
/// 返回 `PidFileGuard` 实例，用于管理PID文件的生命周期
///
/// ## Examples
///
/// ```
/// let guard = write_pid();
/// // 当guard超出作用域时，PID文件会被自动删除
/// ```
pub fn write_pid() -> PidFileGuard {
    debug!("Writing PID...");
    let pid_file_path = get_pid_file_path();
    let pid_file_guard = PidFileGuard {};
    let pid_file = pid_file_path.to_str().unwrap();
    let pid_file = File::create(pid_file).unwrap();
    let mut writer = BufWriter::new(pid_file);
    writer
        .write_all(process::id().to_string().as_bytes())
        .unwrap();
    pid_file_guard
}

fn delete_pid_file_if_my_process() {
    let pid_option = read_pid();
    // 如果 PID 文件存在且是当前进程创建的，则删除
    if let Some(pid) = pid_option
        && pid == process::id() as pid_t
    {
        delete_pid_file();
    }
}

/// # 删除PID文件
///
/// 删除应用程序对应的PID文件。通常在程序正常退出时调用。
///
/// ## Examples
///
/// ```
/// delete_pid(); // 删除PID文件
/// ```
pub(crate) fn delete_pid_file() {
    let pid_file_path = get_pid_file_path();
    info!("Deleting {pid_file_path:?} ...");
    std::fs::remove_file(pid_file_path).unwrap();
}
