//! App 枚举与窗口安置：启动器（App 资源库）的数据层。
//! - 运行中：EnumWindows 枚举顶层窗口（过滤掉工具窗口、无标题、系统外壳和本进程）
//! - 全部：开始菜单「程序」目录的 .lnk（解析出目标 exe）∪ shell 的 AppsFolder
//!   （开始菜单「所有应用」的真身，含没有 .lnk 的打包/UWP 应用）
//! - 最近使用：%APPDATA%\Microsoft\Windows\Recent 的快捷方式按时间倒序，只留 exe 目标
//! - 图标：SHGetFileInfo 取系统图标（对 .lnk 给的是目标程序的图标）；UWP 按 PIDL 取
//! - 全屏到虚拟屏：先还原、SetWindowPos 挪到目标屏、再最大化——窗口会记着新位置；
//!   挪完读回窗口矩形验证中心点落没落在目标屏内，没落位重试一次，结果如实返回给调用方记日志

use crate::display::wide;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use windows::core::{Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HKEY, HWND, LPARAM, POINT, RECT};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, HKEY_CURRENT_USER, KEY_READ,
    REG_BINARY,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER,
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
use windows::Win32::UI::Shell::{
    ApplicationActivationManager, BHID_SFObject, FOLDERID_AppsFolder,
    IApplicationActivationManager, IEnumIDList, ILFree, IShellFolder, IShellItem, IShellLinkW,
    KNOWN_FOLDER_FLAG, SHCONTF_NONFOLDERS, SHGDN_FORPARSING, SHGDN_NORMAL, SHGDNF,
    SHGFI_FLAGS, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_PIDL, SHGetFileInfoW, SHGetKnownFolderItem,
    SHParseDisplayName, ShellExecuteW, ShellLink, StrRetToStrW, ACTIVATEOPTIONS, SHFILEINFOW,
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

/// 一个已安装的 App（开始菜单/AppsFolder 里的一项）。
#[derive(Clone)]
pub struct InstalledApp {
    pub name: String,
    /// 开始菜单快捷方式路径（AppsFolder 独有的项为空）
    pub lnk_path: String,
    /// 解析出的目标 exe（打包/UWP 项为空）
    pub exe_path: String,
    /// 打包/UWP 应用的 AppUserModelID（桌面项为空）
    pub aumid: String,
}

impl InstalledApp {
    /// 启动凭据：优先快捷方式（保留它的参数与工作目录），其次 AUMID，最后 exe 直启。
    pub fn launch_spec(&self) -> String {
        if !self.lnk_path.is_empty() {
            self.lnk_path.clone()
        } else if !self.aumid.is_empty() {
            format!("aumid:{}", self.aumid)
        } else {
            self.exe_path.clone()
        }
    }
    /// 图标凭据：lnk/exe 直接按路径取，UWP 用 aumid: 前缀（按 PIDL 取）。
    pub fn icon_key(&self) -> String {
        if !self.lnk_path.is_empty() {
            self.lnk_path.clone()
        } else if !self.exe_path.is_empty() {
            self.exe_path.clone()
        } else {
            format!("aumid:{}", self.aumid)
        }
    }
    /// 去重键：exe 路径优先，其次 AUMID，退回小写名字。
    pub fn key(&self) -> String {
        if !self.exe_path.is_empty() {
            self.exe_path.to_lowercase()
        } else if !self.aumid.is_empty() {
            format!("aumid:{}", self.aumid.to_lowercase())
        } else {
            self.name.to_lowercase()
        }
    }
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
        // 系统外壳的常驻窗口不是用户 App，不进资源库（实测日志里混进了输入体验宿主）
        if matches!(
            app_name.as_str(),
            "TextInputHost"
                | "ShellExperienceHost"
                | "StartMenuExperienceHost"
                | "SearchHost"
                | "LockApp"
                | "PeopleExperienceHost"
        ) {
            return BOOL(1);
        }
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

// MARK: 窗口尺寸登记与恢复（远程期间被改过的窗口，断开时还原）

/// 一个窗口在远程开始前的样子：普通状态矩形 + 是否最大化。
struct SavedPlace {
    rect: RECT,
    maximized: bool,
    /// 被我们全屏到虚拟屏过：断开时无条件还原。
    /// false 是连入时的普查快照：只还原断开时还停在虚拟屏上的（被用户在远程期间挪过去的）。
    touched: bool,
}

static SAVED_PLACES: Mutex<Option<HashMap<isize, SavedPlace>>> = Mutex::new(None);

/// 读一个窗口当前的普通矩形与最大化状态（WINDOWPLACEMENT 的 rcNormalPosition/showCmd）。
fn current_place(hwnd: HWND) -> Option<(RECT, bool)> {
    unsafe {
        let mut wp = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        GetWindowPlacement(hwnd, &mut wp).ok()?;
        Some((wp.rcNormalPosition, wp.showCmd == SW_SHOWMAXIMIZED.0 as u32))
    }
}

/// 登记一个窗口的原始位置（首次登记为准，之后只补 touched 标记，不覆盖原矩形）。
pub fn record_place(hwnd: isize, touched: bool) {
    let h = HWND(hwnd as *mut core::ffi::c_void);
    unsafe {
        if !IsWindow(Some(h)).as_bool() {
            return;
        }
    }
    let Some((rect, maximized)) = current_place(h) else {
        return;
    };
    let mut guard = SAVED_PLACES.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    match map.get_mut(&hwnd) {
        Some(e) => {
            if touched {
                e.touched = true;
            }
        }
        None => {
            map.insert(
                hwnd,
                SavedPlace {
                    rect,
                    maximized,
                    touched,
                },
            );
        }
    }
}

/// 连入时普查：把当前所有运行中窗口的位置登记为原样（已登记的不覆盖）。
pub fn snapshot_all_windows() {
    for w in running_windows() {
        record_place(w.hwnd, false);
    }
}

/// 拆屏前还原：登记过且（被我们全屏过 || 此刻还停在虚拟屏上）的窗口，
/// 按登记的原矩形与最大化状态用 SetWindowPlacement 还原。还原后清空登记表。
/// 返回实际还原的个数，调用方据此记日志。
pub fn restore_saved_places(virtual_rect: Option<RECT>) -> usize {
    let map = SAVED_PLACES
        .lock()
        .unwrap()
        .take()
        .unwrap_or_default();
    let mut n = 0;
    for (hwnd_raw, saved) in map {
        let h = HWND(hwnd_raw as *mut core::ffi::c_void);
        unsafe {
            if !IsWindow(Some(h)).as_bool() {
                continue;
            }
            let on_virtual = virtual_rect.map(|vr| window_center_in(h, vr)).unwrap_or(false);
            if !saved.touched && !on_virtual {
                continue;
            }
            let wp = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                flags: WINDOWPLACEMENT_FLAGS(0),
                showCmd: if saved.maximized {
                    SW_SHOWMAXIMIZED.0 as u32
                } else {
                    SW_SHOWNORMAL.0 as u32
                },
                ptMinPosition: POINT { x: -1, y: -1 },
                ptMaxPosition: POINT { x: -1, y: -1 },
                rcNormalPosition: saved.rect,
            };
            if SetWindowPlacement(h, &wp).is_ok() {
                n += 1;
            }
        }
    }
    n
}

/// 「全部」列表 = 开始菜单快捷方式（解析出目标 exe）∪ shell 的 AppsFolder。
/// AppsFolder 是开始菜单「所有应用」的真身：没有 .lnk 的打包/UWP 应用只在这里有，
/// 只扫 .lnk 会缺一大块（机主反馈「全部列表感觉不对」的根源）。两边按 exe/AUMID/名字去重。
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
    // 解析每个 .lnk 的目标 exe：运行中判定从此按 exe 精确匹配，不再靠名字猜
    for a in apps.iter_mut() {
        if let Some(t) = resolve_lnk(&a.lnk_path) {
            a.exe_path = t;
        }
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    // AppsFolder 补齐 .lnk 没有的（UWP 等），已有的不重复进
    let mut keys: HashSet<String> = apps.iter().map(|a| a.key()).collect();
    let mut names: HashSet<String> = apps.iter().map(|a| a.name.to_lowercase()).collect();
    for a in apps_folder_apps() {
        if keys.contains(&a.key()) || names.contains(&a.name.to_lowercase()) {
            continue;
        }
        keys.insert(a.key());
        names.insert(a.name.to_lowercase());
        apps.push(a);
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

/// 本线程 COM 初始化（STA）。重复调用、模式冲突都无害，忽略结果。
fn com_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

/// 解析 .lnk 的目标路径（IShellLink + IPersistFile）。失败返回 None。
fn resolve_lnk(lnk: &str) -> Option<String> {
    com_init();
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let pf: IPersistFile = link.cast().ok()?;
        let w = wide(lnk);
        pf.Load(PCWSTR(w.as_ptr()), STGM_READ).ok()?;
        let mut buf = vec![0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..end]);
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

/// 取 shell 文件夹里一项的显示名/解析名（STRRET 转字符串）。
fn display_name(folder: &IShellFolder, pidl: *mut ITEMIDLIST, flags: SHGDNF) -> Option<String> {
    unsafe {
        let mut sr = STRRET::default();
        folder.GetDisplayNameOf(pidl, flags, &mut sr).ok()?;
        let mut pw = PWSTR::null();
        StrRetToStrW(&mut sr, Some(pidl), &mut pw).ok()?;
        if pw.is_null() {
            return None;
        }
        let s = pw.to_string().ok();
        CoTaskMemFree(Some(pw.0 as *const core::ffi::c_void));
        s
    }
}

/// 枚举 shell 的 AppsFolder：桌面应用解析名就是 exe 路径，打包应用是带 ! 的 AUMID；
/// 其余解析名（CLSID、控制面板项等）开始菜单也不会当 App 列，跳过。
fn apps_folder_apps() -> Vec<InstalledApp> {
    com_init();
    let mut out = Vec::new();
    unsafe {
        let Ok(item) = SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KNOWN_FOLDER_FLAG(0), None) else {
            return out;
        };
        let folder: IShellFolder = match item.BindToHandler(None, &BHID_SFObject) {
            Ok(f) => f,
            Err(_) => return out,
        };
        let mut en: Option<IEnumIDList> = None;
        if folder
            .EnumObjects(HWND::default(), SHCONTF_NONFOLDERS.0 as u32, &mut en)
            .is_err()
        {
            return out;
        }
        let Some(en) = en else { return out };
        loop {
            let mut arr: [*mut ITEMIDLIST; 1] = [std::ptr::null_mut()];
            let mut fetched: u32 = 0;
            let hr = en.Next(&mut arr, Some(&mut fetched));
            if hr.is_err() || fetched == 0 || arr[0].is_null() {
                break;
            }
            let pidl = arr[0];
            let name = display_name(&folder, pidl, SHGDN_NORMAL);
            let parsing = display_name(&folder, pidl, SHGDN_FORPARSING);
            CoTaskMemFree(Some(pidl as *const core::ffi::c_void));
            let (Some(name), Some(parsing)) = (name, parsing) else {
                continue;
            };
            let name = name.trim().to_string();
            if name.is_empty()
                || name.starts_with("卸载")
                || name.to_lowercase().starts_with("uninstall")
            {
                continue;
            }
            if parsing.to_lowercase().ends_with(".exe") {
                out.push(InstalledApp {
                    name,
                    lnk_path: String::new(),
                    exe_path: parsing,
                    aumid: String::new(),
                });
            } else if parsing.contains('!') {
                out.push(InstalledApp {
                    name,
                    lnk_path: String::new(),
                    exe_path: String::new(),
                    aumid: parsing,
                });
            }
        }
    }
    out
}

/// 最近使用：读 %APPDATA%\Microsoft\Windows\Recent 的快捷方式（按修改时间倒序），
/// 只留目标是现存 exe 的；能对上已安装列表的复用那一条（启动与图标一致），
/// 对不上的用快捷方式名单独成条。上限 12 条。
pub fn recent_apps(installed: &[InstalledApp]) -> Vec<InstalledApp> {
    let Some(ad) = std::env::var_os("APPDATA") else {
        return Vec::new();
    };
    let dir = PathBuf::from(ad)
        .join("Microsoft")
        .join("Windows")
        .join("Recent");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut lnks: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x.eq_ignore_ascii_case("lnk"))
                .unwrap_or(false)
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    lnks.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out: Vec<InstalledApp> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (_, p) in lnks {
        if out.len() >= 12 {
            break;
        }
        let lnk = p.to_string_lossy().into_owned();
        let Some(target) = resolve_lnk(&lnk) else {
            continue;
        };
        if !target.to_lowercase().ends_with(".exe") || !std::path::Path::new(&target).exists() {
            continue;
        }
        if !seen.insert(target.to_lowercase()) {
            continue;
        }
        if let Some(found) = installed
            .iter()
            .find(|a| a.exe_path.eq_ignore_ascii_case(&target))
        {
            out.push(found.clone());
        } else {
            let name = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            out.push(InstalledApp {
                name,
                lnk_path: lnk,
                exe_path: target,
                aumid: String::new(),
            });
        }
    }
    out
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
                    exe_path: String::new(),
                    aumid: String::new(),
                });
            }
        }
    }
}

/// 取系统图标（大图标）。path 可以是 exe 或 .lnk（.lnk 给目标程序的图标），
/// 或 aumid: 前缀（打包应用按 shell:AppsFolder 的 PIDL 取）。
/// 返回的 HICON 用完要 DestroyIcon。
pub fn icon_for_path(path: &str) -> Option<isize> {
    if let Some(aumid) = path.strip_prefix("aumid:") {
        return icon_for_aumid(aumid);
    }
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

/// 打包应用的图标：解析 shell:AppsFolder\<AUMID> 的 PIDL，再按 PIDL 取图标。
fn icon_for_aumid(aumid: &str) -> Option<isize> {
    com_init();
    unsafe {
        let name = wide(&format!("shell:AppsFolder\\{aumid}"));
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        if SHParseDisplayName(PCWSTR(name.as_ptr()), None, &mut pidl, 0, None).is_err()
            || pidl.is_null()
        {
            return None;
        }
        let mut info = SHFILEINFOW::default();
        let r = SHGetFileInfoW(
            PCWSTR(pidl as *const u16),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_FLAGS(SHGFI_ICON.0 | SHGFI_LARGEICON.0 | SHGFI_PIDL.0),
        );
        ILFree(Some(pidl as *const ITEMIDLIST));
        if r == 0 || info.hIcon.is_invalid() {
            None
        } else {
            Some(info.hIcon.0 as isize)
        }
    }
}

/// 按启动凭据打开 App：aumid: 前缀走打包应用激活，其余按路径 ShellExecute。
pub fn launch_app(spec: &str) -> bool {
    if let Some(aumid) = spec.strip_prefix("aumid:") {
        activate_aumid(aumid)
    } else {
        shell_open(spec)
    }
}

/// 激活打包/UWP 应用（IApplicationActivationManager）。成功返回 true。
fn activate_aumid(aumid: &str) -> bool {
    com_init();
    unsafe {
        let mgr: IApplicationActivationManager = match CoCreateInstance(
            &ApplicationActivationManager,
            None,
            CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER,
        ) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let w = wide(aumid);
        let empty = wide("");
        mgr.ActivateApplication(PCWSTR(w.as_ptr()), PCWSTR(empty.as_ptr()), ACTIVATEOPTIONS(0))
            .is_ok()
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
/// 把窗口全屏到目标屏，并验证真的落位（窗口中心在目标屏内）。
/// 还原/最大化都是异步的：给它一点时间再读回矩形验，没落位重试一次。
/// 返回是否落位——调用方按结果如实记日志，不许假报成功。
pub fn fullscreen_on(hwnd: isize, rect: RECT) -> bool {
    // 动它之前先把原位置登记下来（首次为准），远程断开时按登记还原
    record_place(hwnd, true);
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    unsafe {
        for _ in 0..2 {
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
            std::thread::sleep(std::time::Duration::from_millis(150));
            if window_center_in(hwnd, rect) {
                return true;
            }
        }
        false
    }
}

/// 窗口中心点在不在目标矩形内（最大化后窗口矩形会略大于屏幕，判中心最稳）。
fn window_center_in(hwnd: HWND, rect: RECT) -> bool {
    let mut r = RECT::default();
    unsafe {
        if GetWindowRect(hwnd, &mut r).is_err() {
            return false;
        }
    }
    let cx = (r.left + r.right) / 2;
    let cy = (r.top + r.bottom) / 2;
    cx >= rect.left && cx < rect.right && cy >= rect.top && cy < rect.bottom
}

// MARK: 使用频率（UserAssist）与常用软件

/// 一条使用记录：累计启动次数 + 最近一次启动时间（FILETIME，100ns 单位）。
#[derive(Clone, Copy, Default)]
pub struct UsageInfo {
    pub count: u32,
    pub last: u64,
}

/// UserAssist 的值名是 ROT13 编码的，只转字母。
fn rot13(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='m' | 'A'..='M' => (c as u8 + 13) as char,
            'n'..='z' | 'N'..='Z' => (c as u8 - 13) as char,
            _ => c,
        })
        .collect()
}

/// 文件名主干：去目录、去最后一个扩展名（"C:\a\WINWORD.EXE" → "winword"，已小写输入）。
fn stem_of(path: &str) -> String {
    let base = path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    match base.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => base,
    }
}

/// 从一个解码后的 UserAssist 条目名派生查询键（同一条存多份，查的时候逐个试）：
/// raw: 是整串小写（完整路径 / AUMID / 裸名字），stem: 是文件名主干。
fn usage_keys(decoded: &str) -> Vec<String> {
    let lower = decoded.to_lowercase();
    let mut keys = vec![format!("raw:{lower}")];
    let stem = stem_of(decoded);
    if !stem.is_empty() {
        keys.push(format!("stem:{stem}"));
    }
    keys
}

/// 读注册表 UserAssist（开始菜单「常用应用」背后的数据源）：
/// HKCU\...\Explorer\UserAssist\{GUID}\Count 下每个值，名字是 ROT13 编码的程序
/// 路径/名字，数据是二进制：偏移 4 是启动次数；72 字节新格式偏移 60 是最近
/// 启动的 FILETIME（旧 16 字节格式在偏移 8）。UEME_ 开头的是会话统计条目，
/// 与普通启动条目重复，跳过。同键多条：次数相加、时间取新。
pub fn usage_map() -> HashMap<String, UsageInfo> {
    let mut map: HashMap<String, UsageInfo> = HashMap::new();
    let root_path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\UserAssist");
    unsafe {
        let mut root = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(root_path.as_ptr()),
            None,
            KEY_READ,
            &mut root,
        )
        .0 != 0
        {
            return map;
        }
        let mut gi = 0u32;
        loop {
            let mut gname = [0u16; 128];
            let mut glen = gname.len() as u32;
            let st = RegEnumKeyExW(
                root,
                gi,
                Some(PWSTR(gname.as_mut_ptr())),
                &mut glen,
                None,
                None,
                None,
                None,
            );
            if st.0 != 0 {
                break;
            }
            gi += 1;
            let guid = String::from_utf16_lossy(&gname[..glen as usize]);
            let count_path = wide(&format!("{guid}\\Count"));
            let mut ck = HKEY::default();
            if RegOpenKeyExW(root, PCWSTR(count_path.as_ptr()), None, KEY_READ, &mut ck).0 != 0 {
                continue;
            }
            let mut vi = 0u32;
            loop {
                let mut vname = vec![0u16; 1024];
                let mut vlen = vname.len() as u32;
                let mut vtype = 0u32;
                let mut data = vec![0u8; 256];
                let mut dlen = data.len() as u32;
                let st = RegEnumValueW(
                    ck,
                    vi,
                    Some(PWSTR(vname.as_mut_ptr())),
                    &mut vlen,
                    None,
                    Some(&mut vtype),
                    Some(data.as_mut_ptr()),
                    Some(&mut dlen),
                );
                if st.0 != 0 {
                    break;
                }
                vi += 1;
                if vtype != REG_BINARY.0 || (dlen as usize) < 8 {
                    continue;
                }
                let raw_name = String::from_utf16_lossy(&vname[..vlen as usize]);
                let decoded = rot13(&raw_name);
                if decoded.starts_with("UEME_") || decoded.is_empty() {
                    continue;
                }
                let d = &data[..dlen as usize];
                let count = u32::from_le_bytes(d[4..8].try_into().unwrap());
                let last = if d.len() >= 68 {
                    u64::from_le_bytes(d[60..68].try_into().unwrap())
                } else if d.len() >= 16 {
                    u64::from_le_bytes(d[8..16].try_into().unwrap())
                } else {
                    0
                };
                for k in usage_keys(&decoded) {
                    let e = map.entry(k).or_default();
                    e.count = e.count.saturating_add(count);
                    e.last = e.last.max(last);
                }
            }
            let _ = RegCloseKey(ck);
        }
        let _ = RegCloseKey(root);
    }
    map
}

/// 查一个 App 的使用记录：完整 exe 路径 → AUMID → exe 主干 → 名字，逐个试。
pub fn usage_of(map: &HashMap<String, UsageInfo>, a: &InstalledApp) -> UsageInfo {
    let mut candidates: Vec<String> = Vec::new();
    if !a.exe_path.is_empty() {
        candidates.push(format!("raw:{}", a.exe_path.to_lowercase()));
    }
    if !a.aumid.is_empty() {
        candidates.push(format!("raw:{}", a.aumid.to_lowercase()));
    }
    if !a.exe_path.is_empty() {
        candidates.push(format!("stem:{}", stem_of(&a.exe_path)));
    }
    candidates.push(format!("raw:{}", a.name.to_lowercase()));
    candidates.push(format!("stem:{}", stem_of(&a.name)));
    for k in candidates {
        if let Some(u) = map.get(&k) {
            return *u;
        }
    }
    UsageInfo::default()
}

/// 常用软件加权：没用过的常用软件也能排到长尾前面（机主要求 Office、计算器这类靠前）。
pub const COMMON_BOOST: i64 = 100;

/// 排序分：启动次数 + 常用加权；并列再比最近使用时间。返回 (分数, 最近时间)。
pub fn usage_score(map: &HashMap<String, UsageInfo>, a: &InstalledApp) -> (i64, u64) {
    let u = usage_of(map, a);
    let boost = if is_common_app(a) { COMMON_BOOST } else { 0 };
    (u.count as i64 + boost, u.last)
}

/// 是不是常用软件：按 exe 主干 / 显示名 / AUMID 认。
/// 名单按机主点名的方向收：Office 与 WPS、系统小工具、浏览器、国产办公 IM、开发工具。
pub fn is_common_app(a: &InstalledApp) -> bool {
    const STEMS: &[&str] = &[
        "winword", "excel", "powerpoint", "outlook", "onenote", "msaccess", "mspub", "visio",
        "wps", "et", "wpp", "calc", "calculator", "notepad", "mspaint", "paint", "snippingtool",
        "explorer", "cmd", "powershell", "pwsh", "wt", "windowsterminal", "chrome", "msedge",
        "firefox", "wechat", "wxwork", "dingtalk", "feishu", "lark", "qq", "tim", "wemeetapp",
        "code", "devenv", "idea64", "pycharm64", "vlc", "potplayer", "spotify", "awesun",
        "sunloginclient", "taskmgr", "control",
    ];
    if !a.exe_path.is_empty() && STEMS.contains(&stem_of(&a.exe_path).as_str()) {
        return true;
    }
    const NAME_BITS: &[&str] = &[
        "microsoft word", "microsoft excel", "microsoft powerpoint", "microsoft outlook",
        "microsoft onenote", "microsoft access", "microsoft visio", "wps office",
        "计算器", "calculator", "记事本", "notepad", "画图", "截图工具", "snipping tool",
        "文件资源管理器", "设置", "settings", "微信", "wechat", "企业微信", "钉钉", "dingtalk",
        "飞书", "feishu", "腾讯会议", "向日葵", "sunlogin", "google chrome", "microsoft edge",
        "firefox", "火狐", "visual studio code", "windows terminal", "终端", "命令提示符",
        "powershell", "任务管理器", "task manager", "控制面板", "control panel",
        "spotify", "vlc", "potplayer",
    ];
    let name = a.name.to_lowercase();
    if NAME_BITS.iter().any(|b| name.contains(b)) {
        return true;
    }
    const AUMID_BITS: &[&str] = &[
        "microsoft.office", "windowscalculator", "windowsnotepad", "microsoft.paint",
        "microsoft.screensketch", "microsoft.windowsterminal", "windows.immersivecontrolpanel",
    ];
    let aumid = a.aumid.to_lowercase();
    AUMID_BITS.iter().any(|b| aumid.contains(b))
}
