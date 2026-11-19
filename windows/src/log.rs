//! 日志：写给三个月后排障的人。关键路径都记，失败记原因，不静默。
//! 落盘到 %LOCALAPPDATA%\SnugWindows\snug-windows.log，同时把整行返回给 UI 显示。

use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use windows::Win32::System::SystemInformation::GetLocalTime;

thread_local! {
    static FILE: RefCell<Option<File>> = const { RefCell::new(None) };
}

/// 本应用的数据目录（日志、状态文件都在这里）。
pub fn dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("SnugWindows")
}

pub fn init() {
    let dir = dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("snug-windows.log"))
    {
        FILE.with(|c| *c.borrow_mut() = Some(f));
    }
}

fn stamp() -> String {
    let t = unsafe { GetLocalTime() };
    format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond)
}

/// 写一行日志到文件，返回带时间戳的整行文本。
pub fn write(msg: &str) -> String {
    let line = format!("[{}] {}", stamp(), msg);
    FILE.with(|c| {
        if let Some(f) = c.borrow_mut().as_mut() {
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        }
    });
    line
}
