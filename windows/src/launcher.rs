//! 手机屏上的「App 资源库」（对着 mac 版 PhoneHomeView 的行为）：
//! 盖住整块虚拟屏，两个分页——「运行中」是正在跑的 App（点一下把它的窗口全屏到这块屏），
//! 「全部」是开始菜单里的所有 App（图标网格，运行中的带圆点，点一下打开并全屏过来）。
//! 顶部有搜索框。关掉资源库后，虚拟屏左上角留一个悬浮「App」键，点它再打开——
//! 手机端看不见主窗口，这个键就是回资源库的路。

use crate::apps::{self, InstalledApp, RunningWindow};
use crate::display::wide;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const LAUNCHER_CLASS: &str = "SnugSunloginLauncher";
const HOME_CLASS: &str = "SnugSunloginHomeBtn";
const ID_SEARCH: i32 = 3101;
const EN_CHANGE: u32 = 0x0300;

const TOP_H: i32 = 64;
const CONTENT_TOP: i32 = 76;
const ROW_H: i32 = 76;
const TILE_H: i32 = 158;
const TILE_MIN_W: i32 = 160;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Running,
    All,
}

struct Item {
    name: String,
    sub: String,
    icon_path: String,
    running: bool,
    action: Action,
}

#[derive(Clone)]
enum Action {
    Focus(isize),   // 已有窗口：全屏过去
    Launch(String), // .lnk 路径：打开后等窗口出现再全屏
}

struct LauncherState {
    hwnd: HWND,
    search: HWND,
    tab: Tab,
    running: Vec<RunningWindow>,
    installed: Vec<InstalledApp>,
    query: String,
    scroll: i32,
    rect: RECT,
    icons: HashMap<String, isize>,
    font_title: HFONT,
    font_name: HFONT,
    font_sub: HFONT,
}

struct HomeState {
    hwnd: HWND,
    rect: RECT,
    font: HFONT,
}

thread_local! {
    static LAUNCHER: RefCell<Option<LauncherState>> = const { RefCell::new(None) };
    static HOME: RefCell<Option<HomeState>> = const { RefCell::new(None) };
}

static CLASSES_DONE: AtomicBool = AtomicBool::new(false);

fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | (g << 8) | (b << 16))
}

fn text16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn make_font(height: i32, weight: i32) -> HFONT {
    let mut lf = LOGFONTW::default();
    lf.lfHeight = height;
    lf.lfWeight = weight;
    let face = wide("Segoe UI");
    lf.lfFaceName[..face.len() - 1].copy_from_slice(&face[..face.len() - 1]);
    unsafe { CreateFontIndirectW(&lf) }
}

fn pump(ms: u64) {
    let start = Instant::now();
    let mut msg = MSG::default();
    while start.elapsed() < Duration::from_millis(ms) {
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn hinstance() -> HINSTANCE {
    HINSTANCE(
        unsafe { GetModuleHandleW(None) }
            .map(|m| m.0)
            .unwrap_or_default(),
    )
}

fn register_classes() -> Result<(), String> {
    if CLASSES_DONE.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let classes: [(&str, WNDPROC); 2] = [
        (LAUNCHER_CLASS, Some(launcher_proc)),
        (HOME_CLASS, Some(home_proc)),
    ];
    for (name, proc_) in classes {
        let class_w = wide(name);
        let wc = WNDCLASSW {
            lpfnWndProc: proc_,
            hInstance: hinstance(),
            lpszClassName: PCWSTR(class_w.as_ptr()),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&wc) } == 0 {
            CLASSES_DONE.store(false, Ordering::SeqCst);
            return Err(format!("RegisterClassW({name}) 失败"));
        }
    }
    Ok(())
}

pub fn is_open() -> bool {
    LAUNCHER.with(|c| c.borrow().is_some())
}

// MARK: 打开 / 关闭

/// 在 rect 这块屏上打开资源库（已开则关掉，开关式）。
pub fn toggle(rect: RECT) {
    if is_open() {
        close();
        return;
    }
    if let Err(e) = open(rect) {
        crate::ui_log(&format!("打开资源库失败：{e}"));
    }
}

pub fn open(rect: RECT) -> Result<(), String> {
    register_classes()?;
    hide_home();
    let running = apps::running_windows();
    let installed = apps::installed_apps();
    crate::log::write(&format!(
        "资源库：运行中 {} 个窗口，全部 {} 个 App",
        running.len(),
        installed.len()
    ));
    for w in &running {
        crate::log::write(&format!(
            "  运行中：{}（pid {}）— {}",
            w.app_name, w.pid, w.title
        ));
    }
    LAUNCHER.with(|c| {
        *c.borrow_mut() = Some(LauncherState {
            hwnd: HWND::default(),
            search: HWND::default(),
            tab: Tab::Running,
            running,
            installed,
            query: String::new(),
            scroll: 0,
            rect,
            icons: HashMap::new(),
            font_title: make_font(-38, 700),
            font_name: make_font(-26, 600),
            font_sub: make_font(-22, 400),
        });
    });
    let class_w = wide(LAUNCHER_CLASS);
    let title_w = wide("App 资源库");
    let created = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            WS_POPUP | WS_VISIBLE,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(hinstance()),
            None,
        )
    };
    match created {
        Ok(hwnd) => {
            LAUNCHER.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.hwnd = hwnd;
                }
            });
            Ok(())
        }
        Err(e) => {
            // 创建失败要把状态收干净，不然 is_open 会误报
            LAUNCHER.with(|c| {
                if let Some(s) = c.borrow_mut().take() {
                    unsafe {
                        let _ = DeleteObject(HGDIOBJ(s.font_title.0));
                        let _ = DeleteObject(HGDIOBJ(s.font_name.0));
                        let _ = DeleteObject(HGDIOBJ(s.font_sub.0));
                    }
                }
            });
            Err(format!("CreateWindowExW 失败：{e}"))
        }
    }
}

pub fn close() {
    // 先把 rect 抄出来再销毁：WM_DESTROY 会把状态取走，之后就读不到了
    let (hwnd, rect) = LAUNCHER.with(|c| {
        let cell = c.borrow();
        (
            cell.as_ref().map(|s| s.hwnd),
            cell.as_ref().map(|s| s.rect),
        )
    });
    if let Some(hwnd) = hwnd {
        if !hwnd.is_invalid() {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
    if let Some(rect) = rect {
        show_home(rect);
    }
}

/// 主窗口退出时连悬浮键一起收掉。
pub fn close_all() {
    let hwnd = LAUNCHER.with(|c| c.borrow().as_ref().map(|s| s.hwnd));
    if let Some(hwnd) = hwnd {
        if !hwnd.is_invalid() {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
    let home = HOME.with(|c| c.borrow().as_ref().map(|s| s.hwnd));
    if let Some(hwnd) = home {
        if !hwnd.is_invalid() {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
}

// MARK: 悬浮主页键

fn show_home(rect: RECT) {
    let exists = HOME.with(|c| c.borrow().is_some());
    if exists {
        HOME.with(|c| {
            if let Some(s) = c.borrow_mut().as_mut() {
                s.rect = rect;
                unsafe {
                    let _ = SetWindowPos(
                        s.hwnd,
                        None,
                        rect.left + 18,
                        rect.top + 18,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER,
                    );
                }
            }
        });
    } else {
        if register_classes().is_err() {
            return;
        }
        HOME.with(|c| {
            *c.borrow_mut() = Some(HomeState {
                hwnd: HWND::default(),
                rect,
                font: make_font(-24, 600),
            });
        });
        let class_w = wide(HOME_CLASS);
        let title_w = wide("App");
        let created = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST,
                PCWSTR(class_w.as_ptr()),
                PCWSTR(title_w.as_ptr()),
                WS_POPUP,
                rect.left + 18,
                rect.top + 18,
                76,
                76,
                None,
                None,
                Some(hinstance()),
                None,
            )
        };
        if let Ok(hwnd) = created {
            HOME.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.hwnd = hwnd;
                }
            });
        }
    }
    let hwnd = HOME.with(|c| c.borrow().as_ref().map(|s| s.hwnd));
    if let Some(hwnd) = hwnd {
        if !hwnd.is_invalid() {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
        }
    }
}

fn hide_home() {
    let hwnd = HOME.with(|c| c.borrow().as_ref().map(|s| s.hwnd));
    if let Some(hwnd) = hwnd {
        if !hwnd.is_invalid() {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

unsafe extern "system" fn home_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let brush = CreateSolidBrush(rgb(255, 255, 255));
                let pen = CreatePen(PS_SOLID, 2, rgb(243, 107, 83));
                let old_b = SelectObject(hdc, HGDIOBJ(brush.0));
                let old_p = SelectObject(hdc, HGDIOBJ(pen.0));
                let _ = RoundRect(hdc, 2, 2, rc.right - 2, rc.bottom - 2, 24, 24);
                SelectObject(hdc, old_b);
                SelectObject(hdc, old_p);
                let _ = DeleteObject(HGDIOBJ(brush.0));
                let _ = DeleteObject(HGDIOBJ(pen.0));
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, rgb(35, 35, 40));
                let font = HOME.with(|c| c.borrow().as_ref().map(|s| s.font));
                if let Some(font) = font {
                    let old_f = SelectObject(hdc, HGDIOBJ(font.0));
                    let mut text = text16("App");
                    let mut tr = rc;
                    let _ = DrawTextW(hdc, &mut text, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                    SelectObject(hdc, old_f);
                }
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let rect = HOME.with(|c| c.borrow().as_ref().map(|s| s.rect));
            if let Some(rect) = rect {
                if let Err(e) = open(rect) {
                    crate::log::write(&format!("打开资源库失败：{e}"));
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            // 右键收起悬浮键（主窗口里再点「启动器」可重新打开）
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            HOME.with(|c| {
                if let Some(s) = c.borrow_mut().take() {
                    unsafe {
                        let _ = DeleteObject(HGDIOBJ(s.font.0));
                    }
                }
            });
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

// MARK: 列表数据与布局（布局函数只吃原始参数，绘制/命中/滚动共用一套）

fn matches_query(name: &str, sub: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let q = query.to_lowercase();
    name.to_lowercase().contains(&q) || sub.to_lowercase().contains(&q)
}

/// 当前分页 + 搜索过滤后的条目。
fn current_items(s: &LauncherState) -> Vec<Item> {
    match s.tab {
        Tab::Running => s
            .running
            .iter()
            .filter(|w| matches_query(&w.app_name, &w.title, &s.query))
            .map(|w| Item {
                name: if w.app_name.is_empty() {
                    w.title.clone()
                } else {
                    w.app_name.clone()
                },
                sub: w.title.clone(),
                icon_path: w.exe_path.clone(),
                running: true,
                action: Action::Focus(w.hwnd),
            })
            .collect(),
        Tab::All => s
            .installed
            .iter()
            .filter(|a| matches_query(&a.name, "", &s.query))
            .map(|a| {
                let running = s.running.iter().any(|w| {
                    !w.app_name.is_empty()
                        && (a.name.eq_ignore_ascii_case(&w.app_name)
                            || w.title.to_lowercase().contains(&a.name.to_lowercase()))
                });
                Item {
                    name: a.name.clone(),
                    sub: if running { "运行中".to_string() } else { String::new() },
                    icon_path: a.lnk_path.clone(),
                    running,
                    action: Action::Launch(a.lnk_path.clone()),
                }
            })
            .collect(),
    }
}

fn cols_for(width: i32) -> i32 {
    ((width - 32) / TILE_MIN_W).max(1)
}

fn content_height(tab: Tab, count: usize, width: i32) -> i32 {
    match tab {
        Tab::Running => count as i32 * ROW_H,
        Tab::All => ((count as i32 + cols_for(width) - 1) / cols_for(width)) * TILE_H,
    }
}

fn item_rect(tab: Tab, scroll: i32, index: usize, width: i32) -> RECT {
    match tab {
        Tab::Running => RECT {
            left: 0,
            top: CONTENT_TOP + index as i32 * ROW_H - scroll,
            right: width,
            bottom: CONTENT_TOP + (index as i32 + 1) * ROW_H - scroll,
        },
        Tab::All => {
            let cols = cols_for(width);
            let tile_w = (width - 32) / cols;
            let col = index as i32 % cols;
            let row = index as i32 / cols;
            RECT {
                left: 16 + col * tile_w,
                top: CONTENT_TOP + row * TILE_H - scroll,
                right: 16 + (col + 1) * tile_w,
                bottom: CONTENT_TOP + (row + 1) * TILE_H - scroll,
            }
        }
    }
}

fn tab_rects(width: i32) -> (RECT, RECT, RECT) {
    let mk = |left: i32, right: i32| RECT {
        left,
        top: 13,
        right,
        bottom: 13 + 38,
    };
    (
        mk(width - 348, width - 236),
        mk(width - 228, width - 116),
        mk(width - 108, width - 16),
    )
}

/// 图标缓存（按路径），在状态里；取不到返回 None，绘制侧画占位。
fn icon_cached(path: &str) -> Option<isize> {
    if path.is_empty() {
        return None;
    }
    LAUNCHER.with(|c| {
        let mut cell = c.borrow_mut();
        let s = cell.as_mut()?;
        if let Some(&h) = s.icons.get(path) {
            return Some(h);
        }
        let loaded = apps::icon_for_path(path);
        if let Some(h) = loaded {
            s.icons.insert(path.to_string(), h);
        }
        loaded
    })
}

// MARK: 动作

fn run_action(action: Action, name: &str) {
    let rect = LAUNCHER.with(|c| c.borrow().as_ref().map(|s| s.rect));
    let Some(rect) = rect else { return };
    match action {
        Action::Focus(hwnd) => {
            apps::fullscreen_on(hwnd, rect);
            crate::ui_log(&format!("资源库：{name} 已全屏到虚拟屏"));
            close();
        }
        Action::Launch(lnk) => {
            // 已在运行的同名 App：直接放它的窗口，不重复打开
            let existing = apps::running_windows().into_iter().find(|w| {
                !w.app_name.is_empty()
                    && (name.eq_ignore_ascii_case(&w.app_name)
                        || w.title.to_lowercase().contains(&name.to_lowercase()))
            });
            if let Some(w) = existing {
                apps::fullscreen_on(w.hwnd, rect);
                crate::ui_log(&format!("资源库：{name} 已在运行，窗口已全屏到虚拟屏"));
                close();
                return;
            }
            let before: HashSet<isize> = apps::running_windows().iter().map(|w| w.hwnd).collect();
            if !apps::shell_open(&lnk) {
                crate::ui_log(&format!("资源库：打开 {name} 失败（ShellExecute 拒绝）"));
                return;
            }
            crate::ui_log(&format!("资源库：正在打开 {name}，等它的窗口出现…"));
            close();
            // 等新窗口出现（最多 12 秒）：优先标题/App 名对得上的，其次任意新窗口
            let deadline = Instant::now() + Duration::from_secs(12);
            let mut found: Option<isize> = None;
            while Instant::now() < deadline {
                pump(250);
                let now = apps::running_windows();
                let fresh: Vec<_> = now.iter().filter(|w| !before.contains(&w.hwnd)).collect();
                if let Some(w) = fresh.iter().find(|w| {
                    w.title.to_lowercase().contains(&name.to_lowercase())
                        || name.eq_ignore_ascii_case(&w.app_name)
                }) {
                    found = Some(w.hwnd);
                    break;
                }
                if let Some(w) = fresh.first() {
                    found = Some(w.hwnd);
                    break;
                }
            }
            match found {
                Some(hwnd) => {
                    apps::fullscreen_on(hwnd, rect);
                    crate::ui_log(&format!("资源库：{name} 的窗口已全屏到虚拟屏"));
                }
                None => {
                    crate::ui_log(&format!(
                        "资源库：等了 12 秒没等到 {name} 的新窗口（可能开在了已有窗口里，或启动较慢）"
                    ));
                }
            }
        }
    }
}

// MARK: 绘制

fn draw_text(hdc: HDC, text: &str, rect: &mut RECT, format: DRAW_TEXT_FORMAT) {
    let mut buf = text16(text);
    unsafe {
        let _ = DrawTextW(hdc, &mut buf, rect, format);
    }
}

fn paint(hwnd: HWND) {
    // 先把要画的东西快照出来：取图标时要再借一次状态，不能攥着借用画
    let snap = LAUNCHER.with(|c| {
        let cell = c.borrow();
        cell.as_ref().map(|s| {
            (
                current_items(s),
                s.tab,
                s.scroll,
                s.font_title,
                s.font_name,
                s.font_sub,
            )
        })
    });
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let Some((items, tab, scroll, font_title, font_name, font_sub)) = snap else {
            let _ = EndPaint(hwnd, &ps);
            return;
        };
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        if w > 0 && h > 0 {
            let mem = CreateCompatibleDC(Some(hdc));
            let bmp = CreateCompatibleBitmap(hdc, w, h);
            let old_bmp = SelectObject(mem, HGDIOBJ(bmp.0));
            let bg = CreateSolidBrush(rgb(246, 246, 248));
            FillRect(mem, &rc, bg);
            let _ = DeleteObject(HGDIOBJ(bg.0));

            // 页眉：标题 + 分页 + 关闭
            SetBkMode(mem, TRANSPARENT);
            SetTextColor(mem, rgb(30, 30, 34));
            let old_font = SelectObject(mem, HGDIOBJ(font_title.0));
            let mut tr = RECT {
                left: 20,
                top: 8,
                right: 110,
                bottom: TOP_H,
            };
            draw_text(mem, "App", &mut tr, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            SelectObject(mem, HGDIOBJ(font_name.0));
            let (run_rc, all_rc, close_rc) = tab_rects(w);
            for (rcx, label, active) in [
                (run_rc, "运行中", tab == Tab::Running),
                (all_rc, "全部", tab == Tab::All),
                (close_rc, "关闭", false),
            ] {
                let fill = if active { rgb(38, 38, 43) } else { rgb(226, 226, 231) };
                let brush = CreateSolidBrush(fill);
                let pen = CreatePen(PS_SOLID, 1, fill);
                let ob = SelectObject(mem, HGDIOBJ(brush.0));
                let op = SelectObject(mem, HGDIOBJ(pen.0));
                let _ = RoundRect(mem, rcx.left, rcx.top, rcx.right, rcx.bottom, 18, 18);
                SelectObject(mem, ob);
                SelectObject(mem, op);
                let _ = DeleteObject(HGDIOBJ(brush.0));
                let _ = DeleteObject(HGDIOBJ(pen.0));
                SetTextColor(mem, if active { rgb(255, 255, 255) } else { rgb(30, 30, 34) });
                let mut trc = rcx;
                draw_text(mem, label, &mut trc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            }
            SetTextColor(mem, rgb(30, 30, 34));

            if items.is_empty() {
                SelectObject(mem, HGDIOBJ(font_name.0));
                SetTextColor(mem, rgb(130, 130, 138));
                let mut tr = RECT {
                    left: 0,
                    top: CONTENT_TOP + 60,
                    right: w,
                    bottom: CONTENT_TOP + 110,
                };
                let msg = match tab {
                    Tab::Running => "没有正在运行的 App",
                    Tab::All => "没有找到 App",
                };
                draw_text(mem, msg, &mut tr, DT_CENTER | DT_SINGLELINE);
            }
            for (i, item) in items.iter().enumerate() {
                let r = item_rect(tab, scroll, i, w);
                if r.bottom < CONTENT_TOP || r.top > h {
                    continue;
                }
                let icon = icon_cached(&item.icon_path);
                match tab {
                    Tab::Running => {
                        if let Some(hicon) = icon {
                            let _ = DrawIconEx(
                                mem,
                                24,
                                r.top + 14,
                                HICON(hicon as *mut core::ffi::c_void),
                                48,
                                48,
                                0,
                                None,
                                DI_NORMAL,
                            );
                        }
                        SelectObject(mem, HGDIOBJ(font_name.0));
                        SetTextColor(mem, rgb(30, 30, 34));
                        let mut tr = RECT {
                            left: 92,
                            top: r.top + 8,
                            right: w - 20,
                            bottom: r.top + 42,
                        };
                        draw_text(
                            mem,
                            &item.name,
                            &mut tr,
                            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                        );
                        SelectObject(mem, HGDIOBJ(font_sub.0));
                        SetTextColor(mem, rgb(125, 125, 133));
                        let mut tr = RECT {
                            left: 92,
                            top: r.top + 40,
                            right: w - 20,
                            bottom: r.top + 70,
                        };
                        draw_text(
                            mem,
                            &item.sub,
                            &mut tr,
                            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                        );
                    }
                    Tab::All => {
                        let tile_w = (w - 32) / cols_for(w);
                        let cx = r.left + tile_w / 2;
                        if let Some(hicon) = icon {
                            let _ = DrawIconEx(
                                mem,
                                cx - 32,
                                r.top + 14,
                                HICON(hicon as *mut core::ffi::c_void),
                                64,
                                64,
                                0,
                                None,
                                DI_NORMAL,
                            );
                        }
                        SelectObject(mem, HGDIOBJ(font_sub.0));
                        SetTextColor(mem, rgb(30, 30, 34));
                        let mut tr = RECT {
                            left: r.left + 6,
                            top: r.top + 88,
                            right: r.right - 6,
                            bottom: r.top + 138,
                        };
                        draw_text(mem, &item.name, &mut tr, DT_CENTER | DT_WORDBREAK);
                        if item.running {
                            let dot = CreateSolidBrush(rgb(243, 107, 83));
                            let old = SelectObject(mem, HGDIOBJ(dot.0));
                            let _ = Ellipse(mem, cx - 4, r.top + 142, cx + 4, r.top + 150);
                            SelectObject(mem, old);
                            let _ = DeleteObject(HGDIOBJ(dot.0));
                        }
                    }
                }
            }
            SelectObject(mem, old_font);
            let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
            SelectObject(mem, old_bmp);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

// MARK: 窗口过程

unsafe extern "system" fn launcher_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let rect = LAUNCHER.with(|c| {
                c.borrow()
                    .as_ref()
                    .map(|s| s.rect)
                    .unwrap_or_default()
            });
            let w = rect.right - rect.left;
            let class_w = wide("Edit");
            let search = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    PCWSTR(class_w.as_ptr()),
                    PCWSTR::null(),
                    WS_CHILD | WS_VISIBLE | WS_BORDER,
                    116,
                    14,
                    (w - 480).max(120),
                    36,
                    Some(hwnd),
                    Some(HMENU(ID_SEARCH as *mut core::ffi::c_void)),
                    Some(hinstance()),
                    None,
                )
            }
            .unwrap_or_default();
            LAUNCHER.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.search = search;
                }
            });
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 as u32 & 0xffff) as i32;
            let notify = (wparam.0 as u32 >> 16) as u32;
            if id == ID_SEARCH && notify == EN_CHANGE {
                let text = LAUNCHER.with(|c| {
                    let cell = c.borrow();
                    cell.as_ref().map(|s| {
                        let len = unsafe { GetWindowTextLengthW(s.search) };
                        let mut buf = vec![0u16; len as usize + 1];
                        let got = unsafe { GetWindowTextW(s.search, &mut buf) };
                        String::from_utf16_lossy(&buf[..got as usize])
                    })
                });
                if let Some(text) = text {
                    LAUNCHER.with(|c| {
                        if let Some(s) = c.borrow_mut().as_mut() {
                            s.query = text.trim().to_string();
                            s.scroll = 0;
                        }
                    });
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 as u32 >> 16) as u16) as i16 as i32;
            LAUNCHER.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    let items = current_items(s);
                    let mut rc = RECT::default();
                    unsafe {
                        let _ = GetClientRect(hwnd, &mut rc);
                    }
                    let max_scroll = (content_height(s.tab, items.len(), rc.right - rc.left)
                        - (rc.bottom - rc.top - CONTENT_TOP))
                        .max(0);
                    s.scroll = (s.scroll - delta / 120 * 57).clamp(0, max_scroll);
                }
            });
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 as u32 & 0xffff) as i16 as i32;
            let y = ((lparam.0 as u32 >> 16) as u16) as i16 as i32;
            let mut rc = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut rc);
            }
            let w = rc.right - rc.left;
            if y < TOP_H {
                let (run_rc, all_rc, close_rc) = tab_rects(w);
                let inside = |r: RECT| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
                if inside(run_rc) || inside(all_rc) {
                    let to_running = inside(run_rc);
                    LAUNCHER.with(|c| {
                        if let Some(s) = c.borrow_mut().as_mut() {
                            s.tab = if to_running { Tab::Running } else { Tab::All };
                            s.scroll = 0;
                        }
                    });
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                } else if inside(close_rc) {
                    close();
                }
                return LRESULT(0);
            }
            let hit = LAUNCHER.with(|c| {
                let cell = c.borrow();
                let s = cell.as_ref()?;
                let items = current_items(s);
                items.iter().enumerate().find_map(|(i, item)| {
                    let r = item_rect(s.tab, s.scroll, i, w);
                    if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                        Some((item.action.clone(), item.name.clone()))
                    } else {
                        None
                    }
                })
            });
            if let Some((action, name)) = hit {
                run_action(action, &name);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            LAUNCHER.with(|c| {
                if let Some(s) = c.borrow_mut().take() {
                    unsafe {
                        for (_, hicon) in s.icons {
                            let _ = DestroyIcon(HICON(hicon as *mut core::ffi::c_void));
                        }
                        let _ = DeleteObject(HGDIOBJ(s.font_title.0));
                        let _ = DeleteObject(HGDIOBJ(s.font_name.0));
                        let _ = DeleteObject(HGDIOBJ(s.font_sub.0));
                    }
                }
            });
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
