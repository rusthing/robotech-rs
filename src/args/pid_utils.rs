use crate::env::{Env, ENV};
use libc::pid_t;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process;

pub fn read_pid() -> Option<pid_t> {
    let Env { app_file_path, .. } = ENV.get().expect("ENV is None");
    let mut pid_file = app_file_path.clone();
    pid_file.add_extension("pid");
    if !pid_file.exists() {
        return None;
    }
    let pid_file = pid_file.to_str().unwrap();
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

pub fn write_pid() {
    let Env { app_file_path, .. } = ENV.get().expect("ENV is None");
    let pid = process::id();
    let mut pid_file = app_file_path.clone();
    pid_file.add_extension("pid");
    let pid_file = pid_file.to_str().unwrap();
    let pid_file = File::create(pid_file).unwrap();
    let mut writer = BufWriter::new(pid_file);
    writer.write_all(pid.to_string().as_bytes()).unwrap();
}
