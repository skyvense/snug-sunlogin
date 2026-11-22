//! 向日葵会话监视：桌面主控连入时，向日葵会拉起带 --mod=desktopagent 的桌面代理进程，
//! 它在不在就是「有没有人正在桌面控制本机」（传文件、看摄像头走的是别的代理，不会误触发）。
//! 后台线程轮询进程命令行，只在出现/消失的边沿给主窗口发消息，
//! 建屏/拆屏的实际动作都在 UI 线程做。消失要连续两次查不到才算断开，
//! 防代理进程重启换代时闪断误拆。

use crate::lang::tr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_SESSION_START: u32 = WM_APP + 10;
pub const WM_SESSION_END: u32 = WM_APP + 11;

/// 主控当前是否连着（监视线程每次轮询都更新，UI 线程读它拼状态文案）。
pub static AGENT_PRESENT: AtomicBool = AtomicBool::new(false);

fn agent_running() -> bool {
    // 全量查进程命令行找标记：先按进程名粗筛会漏掉改名后的新版客户端，全量最稳。
    // 每 3 秒一次、只在后台线程跑，开销可接受；真嫌重再换原生进程枚举。
    // 必须排除查询进程自己（$PID）：脚本是经 -Command 传给 powershell.exe 的，
    // 标记字符串就写在它自己的命令行里，不排除的话每次都会「查到」自己，
    // 会话被永远判为连着、断开永远触发不了（2026-11-20 真机实测踩中）。
    let script = "Get-CimInstance Win32_Process | Where-Object { $_.ProcessId -ne $PID -and $_.CommandLine -like '*--mod=desktopagent*' } | Select-Object -First 1 -ExpandProperty ProcessId";
    let (exit, output) = crate::devices::run(
        "powershell.exe",
        &["-NoProfile", "-Command", script],
    );
    exit == 0 && !output.trim().is_empty()
}

pub fn spawn(hwnd_raw: usize) {
    std::thread::spawn(move || {
        let mut present = false;
        let mut misses = 0u32;
        loop {
            let now = agent_running();
            AGENT_PRESENT.store(now, Ordering::Relaxed);
            if now {
                misses = 0;
                if !present {
                    present = true;
                    crate::log::write(tr("会话监视：desktopagent 出现，通知主窗口（连入）", "Session watch: desktopagent appeared, notifying the main window (connected)"));
                    post(hwnd_raw, WM_SESSION_START);
                }
            } else if present {
                misses += 1;
                if misses >= 2 {
                    present = false;
                    misses = 0;
                    crate::log::write(tr("会话监视：desktopagent 连续两次查不到，通知主窗口（断开）", "Session watch: desktopagent missing twice in a row, notifying the main window (disconnected)"));
                    post(hwnd_raw, WM_SESSION_END);
                }
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
}

fn post(hwnd_raw: usize, msg: u32) {
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd_raw as *mut core::ffi::c_void)),
            msg,
            WPARAM(0),
            LPARAM(0),
        );
    }
}
