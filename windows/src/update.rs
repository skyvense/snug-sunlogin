//! 自动更新：向 GitHub 查最新 Release，和编进程序的版本号（仓库根 VERSION）比对；
//! 有新版先问用户，同意了才下载 zip、按官方 .sha256 验包、解压，最后留一个换文件
//! 脚本等本进程退出后替换 exe 并重新拉起（运行中的 exe 覆盖不了自己）。
//! 查询与下载都借 PowerShell（它解析 JSON、算哈希都现成），Rust 侧不引 HTTP 依赖。

use std::path::PathBuf;
use std::sync::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_UPDATE_AVAILABLE: u32 = WM_APP + 20;
pub const WM_UPDATE_NONE: u32 = WM_APP + 21;
pub const WM_UPDATE_FAILED: u32 = WM_APP + 22;
pub const WM_UPDATE_READY: u32 = WM_APP + 23;

const RELEASES_API: &str =
    "https://api.github.com/repos/skyvense/snug-sunlogin/releases/latest";

#[derive(Clone)]
pub struct Latest {
    /// 不带 v 前缀，如 "2.6.4"
    pub version: String,
    pub zip_url: String,
    pub sha_url: String,
}

static PENDING: Mutex<Option<Latest>> = Mutex::new(None);
static FAILURE: Mutex<Option<String>> = Mutex::new(None);
static SWAP_JOB: Mutex<Option<(PathBuf, PathBuf, PathBuf)>> = Mutex::new(None);

/// 本机版本号：构建时 build.rs 从仓库根 VERSION 读入
pub fn current_version() -> &'static str {
    env!("SNUG_VERSION")
}

fn newer(current: &str, latest: &str) -> bool {
    let parse = |s: &str| {
        s.trim()
            .trim_start_matches('v')
            .split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    parse(latest) > parse(current)
}

fn post(hwnd_raw: usize, msg: u32) {
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd_raw as *mut _)),
            msg,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

/// 后台查新版。manual（用户点了「检查更新」）时没新版/失败也回消息让界面给个说法；
/// 启动时的自动检查只在有新版时才打扰用户。
pub fn spawn_check(hwnd_raw: usize, manual: bool) {
    std::thread::spawn(move || match fetch_latest() {
        Ok(latest) if newer(current_version(), &latest.version) => {
            *PENDING.lock().unwrap() = Some(latest);
            post(hwnd_raw, WM_UPDATE_AVAILABLE);
        }
        Ok(_) => {
            if manual {
                post(hwnd_raw, WM_UPDATE_NONE);
            }
        }
        Err(e) => {
            crate::log::write(&format!("检查更新失败：{e}"));
            if manual {
                *FAILURE.lock().unwrap() = Some(e);
                post(hwnd_raw, WM_UPDATE_FAILED);
            }
        }
    });
}

fn fetch_latest() -> Result<Latest, String> {
    // PowerShell 解析 JSON 后只吐一行「版本|zip 地址|sha 地址」
    let script = format!(
        "$r = Invoke-RestMethod '{RELEASES_API}' -Headers @{{ 'User-Agent' = 'SnugWindows' }} -TimeoutSec 20; \
         $zip = $r.assets | Where-Object {{ $_.name -like 'Snug-Sunlogin-Windows-*.zip' }} | Select-Object -First 1; \
         $sha = $r.assets | Where-Object {{ $_.name -eq ($zip.name + '.sha256') }} | Select-Object -First 1; \
         $r.tag_name + '|' + $zip.browser_download_url + '|' + $sha.browser_download_url"
    );
    let (exit, output) = crate::devices::run(
        "powershell.exe",
        &["-NoProfile", "-Command", &script],
    );
    let line = output
        .lines()
        .map(str::trim)
        .find(|l| l.contains('|'))
        .ok_or_else(|| format!("查询发布失败（退出码 {exit}）：{}", output.trim()))?;
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() != 3 || parts[1].is_empty() || parts[2].is_empty() {
        return Err(format!("发布信息不完整：{line}"));
    }
    Ok(Latest {
        version: parts[0].trim().trim_start_matches('v').to_string(),
        zip_url: parts[1].trim().to_string(),
        sha_url: parts[2].trim().to_string(),
    })
}

pub fn take_pending() -> Option<Latest> {
    PENDING.lock().unwrap().take()
}

pub fn take_failure() -> Option<String> {
    FAILURE.lock().unwrap().take()
}

/// 后台下载新版 zip、按 .sha256 验包、解压，并写好换文件脚本；
/// 成功回 WM_UPDATE_READY，界面随后拉起脚本并退出本进程。
pub fn spawn_prepare(hwnd_raw: usize, latest: Latest) {
    std::thread::spawn(move || match prepare(&latest) {
        Ok(()) => post(hwnd_raw, WM_UPDATE_READY),
        Err(e) => {
            crate::log::write(&format!("准备更新失败：{e}"));
            *FAILURE.lock().unwrap() = Some(e);
            post(hwnd_raw, WM_UPDATE_FAILED);
        }
    });
}

fn prepare(latest: &Latest) -> Result<(), String> {
    let dir = std::env::temp_dir().join("SnugWindowsUpdate");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("建临时目录失败：{e}"))?;
    let zip = dir.join("update.zip");
    let sha = dir.join("update.zip.sha256");
    let out = dir.join("new");
    let script = format!(
        "Invoke-WebRequest '{zip_url}' -OutFile '{zip}' -TimeoutSec 600; \
         Invoke-WebRequest '{sha_url}' -OutFile '{sha}' -TimeoutSec 60; \
         $want = ((Get-Content '{sha}') -split '\\s+')[0].ToUpper(); \
         $got = (Get-FileHash '{zip}' -Algorithm SHA256).Hash.ToUpper(); \
         if ($want -ne $got) {{ throw \"SHA256 对不上，期望 $want 实际 $got\" }}; \
         Expand-Archive '{zip}' -DestinationPath '{out}' -Force; 'DONE'",
        zip_url = latest.zip_url,
        sha_url = latest.sha_url,
        zip = zip.display(),
        sha = sha.display(),
        out = out.display(),
    );
    let (exit, output) = crate::devices::run(
        "powershell.exe",
        &["-NoProfile", "-Command", &script],
    );
    if exit != 0 || !output.contains("DONE") {
        return Err(format!(
            "下载/验包/解压失败（退出码 {exit}）：{}",
            output.trim()
        ));
    }
    // 换文件脚本：等本进程退出 → 覆盖 exe 与安装脚本 → 重新拉起。
    // 本进程是管理员权限，拉起的 PowerShell 子进程继承权限，覆盖无需再提权。
    // 发布 zip 里有一层版本目录，exe 不在解压根上，要递归找到它所在目录再整套换
    let swap = "param([int]$OldPid, [string]$Src, [string]$Dst)\n\
        Wait-Process -Id $OldPid -Timeout 30 -ErrorAction SilentlyContinue\n\
        Start-Sleep -Milliseconds 600\n\
        $exe = Get-ChildItem $Src -Recurse -Filter 'SnugWindows.exe' | Select-Object -First 1\n\
        if (-not $exe) { return }\n\
        $from = $exe.Directory.FullName\n\
        Copy-Item $exe.FullName (Join-Path $Dst 'SnugWindows.exe') -Force\n\
        $s = Join-Path $from 'setup-virtual-display.ps1'\n\
        if (Test-Path $s) { Copy-Item $s (Join-Path $Dst 'setup-virtual-display.ps1') -Force }\n\
        foreach ($d in @('vdd', 'nefcon')) {\n\
        $sd = Join-Path $from $d\n\
        if (Test-Path $sd) { Copy-Item $sd (Join-Path $Dst $d) -Recurse -Force }\n\
        }\n\
        Start-Process (Join-Path $Dst 'SnugWindows.exe')\n";
    let swap_path = dir.join("swap.ps1");
    std::fs::write(&swap_path, swap).map_err(|e| format!("写换文件脚本失败：{e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("找不到本程序路径：{e}"))?;
    let dst = exe
        .parent()
        .ok_or("本程序不在任何目录里")?
        .to_path_buf();
    *SWAP_JOB.lock().unwrap() = Some((swap_path, out, dst));
    Ok(())
}

/// 拉起换文件脚本（独立进程，不等它结束）；界面紧接着退出本进程，脚本等进程
/// 退出后换文件并重新打开新版。
pub fn launch_swap() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let (swap, src, dst) = SWAP_JOB
        .lock()
        .unwrap()
        .take()
        .ok_or("没有待执行的更新")?;
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
            &swap.to_string_lossy(),
            &std::process::id().to_string(),
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
        ])
        .creation_flags(0x0000_0008) // DETACHED_PROCESS：不随本进程退出被带走
        .spawn()
        .map_err(|e| format!("拉起换文件脚本失败：{e}"))?;
    Ok(())
}
