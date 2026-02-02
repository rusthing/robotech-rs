use crate::env::{Env, ENV};
use libc::pid_t;
use log::info;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process;

/// PID 文件守卫结构
/// 
/// 用于管理 PID 文件的生命周期，通过 RAII 模式在程序退出时自动删除 PID 文件。
/// 
/// # 使用示例
/// 
/// ```
/// let guard = write_pid();  // 创建 PID 文件并返回守卫
/// // 当 guard 超出作用域时，会自动删除 PID 文件
/// ```
pub struct PidFileGuard;

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        delete_pid_of_my_process();
    }
}

fn get_pid_file_path() -> PathBuf {
    let Env { app_file_path, .. } = ENV.get().expect("ENV is None");
    let mut pid_file_path = app_file_path.clone();
    pid_file_path.add_extension("pid");
    pid_file_path
}

pub fn read_pid() -> Option<pid_t> {
    info!("Reading PID...");
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

pub fn write_pid() -> PidFileGuard {
    info!("Writing PID...");
    let pid_file_path = get_pid_file_path();
    let pid_file_guard = create_pid_file_guard();
    let pid_file = pid_file_path.to_str().unwrap();
    let pid_file = File::create(pid_file).unwrap();
    let mut writer = BufWriter::new(pid_file);
    writer
        .write_all(process::id().to_string().as_bytes())
        .unwrap();
    pid_file_guard
}

fn delete_pid_of_my_process() {
    let pid_option = read_pid();
    // 如果 PID 文件存在且是当前进程创建的，则删除
    if let Some(pid) = pid_option
        && pid == process::id() as pid_t
    {
        let pid_file_path = get_pid_file_path();
        info!("Deleting {pid_file_path:?} ...");
        std::fs::remove_file(pid_file_path).unwrap();
    }
}

pub fn delete_pid() {
    let pid_file_path = get_pid_file_path();
    info!("Deleting {pid_file_path:?} ...");
    std::fs::remove_file(pid_file_path).unwrap();
}

fn create_pid_file_guard() -> PidFileGuard {
    PidFileGuard {}
}
