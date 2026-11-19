//! App 枚举与窗口安置：启动器（App 资源库）的数据层。
//! - 运行中：EnumWindows 枚举顶层窗口（过滤掉工具窗口、无标题、系统外壳和本进程）
//! - 全部：扫开始菜单「程序」目录（系统级 + 用户级）的 .lnk，就是开始菜单里的「所有应用」
//! - 图标：SHGetFileInfo 取系统图标（对 .lnk 给的是目标程序的图标）
//! - 全屏到虚拟屏：先还原、SetWindowPos 挪到目标屏、再最大化——窗口会记着新位置

use crate::display::wide;
use std::path::PathBuf;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{
    SHGetFileInfoW, ShellExecuteW, SHFILEINFOW, SHGFI_FLAGS, SHGFI_ICON, SHGFI_LARGEICON,
};
use windows::Win32::UI::WindowsAndMessaging::*;

/// 一个正在运行的 App 的顶层窗口。
#[derive(Clone)]
pub struct RunningWindow {
    pub hwnd: isize,
    pub title: String,
    pub exe_path: String,
    pub app_name: String, // exe 文件名去扩展名
    pub pid: u32,
}

/// 一个已安装的 App（开始菜单里的一项）。
#[derive(Clone)]
pub struct InstalledApp {
    pub name: String,
    pub lnk_path: String,
}

const WS_EX_TOOLWINDOW: i32 = 0x0000_0080;

extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = unsafe { &mut *(lparam.0 as *mut Vec<RunningWindow>) };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        // 只收根窗口（无 owner 的顶层窗口）
        if GetAncestor(hwnd, GA_ROOT) != hwnd {
            return BOOL(1);
        }
        if GetWindowLongW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW != 0 {
            return BOOL(1);
        }
        let mut class_buf = [0u16; 64];
        let class_len = GetClassNameW(hwnd, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);
        if matches!(
            class_name.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "DV2ControlHost"
        ) {
            return BOOL(1);
        }
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 {
            return BOOL(1);
        }
        let mut title_buf = vec![0u16; len as usize + 1];
        let got = GetWindowTextW(hwnd, &mut title_buf);
        let title = String::from_utf16_lossy(&title_buf[..got as usize]);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() || pid == 0 {
            return BOOL(1);
        }
        let exe_path = process_path(pid).unwrap_or_default();
        let app_name = exe_path
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .rsplit('.')
            .last()
            .unwrap_or("")
            .to_string();
        out.push(RunningWindow {
            hwnd: hwnd.0 as isize,
            title,
            exe_path,
            app_name,
            pid,
        });
    }
    BOOL(1)
}

fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// 枚举正在运行的 App 窗口（顺序即系统枚举顺序，调用方按需排序）。
pub fn running_windows() -> Vec<RunningWindow> {
    let mut out: Vec<RunningWindow> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_proc),
            LPARAM(&mut out as *mut Vec<RunningWindow> as isize),
        );
    }
    out
}

/// 扫开始菜单「程序」目录里的全部 .lnk（递归，深度上限 4），按名字排序、去重。
pub fn installed_apps() -> Vec<InstalledApp> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(pd) = std::env::var_os("ProgramData") {
        roots.push(
            PathBuf::from(pd)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    if let Some(ad) = std::env::var_os("APPDATA") {
        roots.push(
            PathBuf::from(ad)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    let mut apps: Vec<InstalledApp> = Vec::new();
    for root in &roots {
        collect_lnk(root, 0, &mut apps);
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    apps
}

fn collect_lnk(dir: &std::path::Path, depth: u32, out: &mut Vec<InstalledApp>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_lnk(&path, depth + 1, out);
        } else if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("lnk"))
            .unwrap_or(false)
        {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let name = stem.trim();
                // 卸载项、文档类快捷方式不是 App，跳过（与开始菜单的观感一致地收敛）
                if name.is_empty() || name.to_lowercase().starts_with("uninstall") {
                    continue;
                }
                out.push(InstalledApp {
                    name: name.to_string(),
                    lnk_path: path.to_string_lossy().into_owned(),
                });
            }
        }
    }
}

/// 取系统图标（大图标）。path 可以是 exe 或 .lnk（.lnk 给目标程序的图标）。
/// 返回的 HICON 用完要 DestroyIcon。
pub fn icon_for_path(path: &str) -> Option<isize> {
    let path_w = wide(path);
    let mut info = SHFILEINFOW::default();
    let r = unsafe {
        SHGetFileInfoW(
            PCWSTR(path_w.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_FLAGS(SHGFI_ICON.0 | SHGFI_LARGEICON.0),
        )
    };
    if r == 0 || info.hIcon.is_invalid() {
        None
    } else {
        Some(info.hIcon.0 as isize)
    }
}

/// 打开一个 .lnk（或任意文件）。成功返回 true。
pub fn shell_open(path: &str) -> bool {
    let path_w = wide(path);
    let op_w = wide("open");
    let r = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op_w.as_ptr()),
            PCWSTR(path_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    r.0 as isize > 32
}

/// 把窗口全屏（最大化）到 rect 这块屏上：最小化/最大化先还原，
/// SetWindowPos 挪过去并按目标尺寸放好，再最大化让它记住这块屏。
pub fn fullscreen_on(hwnd: isize, rect: RECT) {
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    unsafe {
        if IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER,
        );
        let _ = ShowWindow(hwnd, SW_MAXIMIZE);
        let _ = SetForegroundWindow(hwnd);
    }
}
