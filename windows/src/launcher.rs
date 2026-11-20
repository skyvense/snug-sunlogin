//! 手机屏上的「App 资源库」（对着 mac 版 PhoneHomeView 的行为）：
//! 盖住整块虚拟屏，两个分页——「运行中」按 App 分组（点行把它的窗口全屏到这块屏；
//! 多窗口的 App 行尾有展开钮，展开后逐个窗口选、逐个窗口关；单窗口行尾有 ✕ 直接关窗），
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
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

const LAUNCHER_CLASS: &str = "SnugSunloginLauncher";
const HOME_CLASS: &str = "SnugSunloginHomeBtn";
const ID_SEARCH: i32 = 3101;
const EN_CHANGE: u32 = 0x0300;

const TOP_H: i32 = 64;
const CONTENT_TOP: i32 = 76;
const ROW_H: i32 = 88;
const TILE_H: i32 = 188;
const TILE_MIN_W: i32 = 172;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Running,
    All,
}

#[derive(Clone, Copy, PartialEq)]
enum RowKind {
    /// 「运行中」的 App 分组行：点击全屏它的第一个窗口。
    AppGroup,
    /// 分组展开后的单个窗口行（缩进显示）：点击全屏这个窗口，行尾 ✕ 关这个窗口。
    WindowRow,
    /// 「全部」的已安装 App。
    Installed,
}

struct Item {
    name: String,
    sub: String,
    icon_path: String,
    running: bool,
    action: Action,
    kind: RowKind,
    /// 行尾 ✕ 能关的窗口（单窗口行有；多窗口分组行没有——关窗在展开的子行里逐个关）。
    close_hwnd: Option<isize>,
    /// 所属 App 分组键（展开状态按它记）；「全部」行为空。
    app_key: String,
    /// 分组行的窗口数（画展开钮用）。
    win_count: usize,
    expanded: bool,
    /// 这条 App 的目标 exe（运行中精确匹配用；窗口行/分组行也带自己的 exe）。
    exe_path: String,
}

#[derive(Clone)]
enum Action {
    Focus(isize),   // 已有窗口：全屏过去
    Launch(String), // 启动凭据（.lnk 路径 / aumid: 前缀 / exe 路径）：打开后等窗口出现再全屏
}

struct LauncherState {
    hwnd: HWND,
    search: HWND,
    tab: Tab,
    running: Vec<RunningWindow>,
    installed: Vec<InstalledApp>,
    /// 最近使用（Recent 目录解析出的 App 子集，无搜索时在「全部」置顶）。
    recent: Vec<InstalledApp>,
    query: String,
    scroll: i32,
    rect: RECT,
    /// 「运行中」里被展开的 App 分组键集合。
    expanded: HashSet<String>,
    icons: HashMap<String, isize>,
    font_title: HFONT,
    font_name: HFONT,
    font_sub: HFONT,
}

struct HomeState {
    hwnd: HWND,
    rect: RECT,
    /// 手指/鼠标正按着：绘制按压态（缩小 + 变暗），抬起才触发打开资源库。
    pressed: bool,
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
    let recent = apps::recent_apps(&installed);
    crate::log::write(&format!(
        "资源库：运行中 {} 个窗口，全部 {} 个 App，最近使用 {} 个",
        running.len(),
        installed.len(),
        recent.len()
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
            recent,
            query: String::new(),
            scroll: 0,
            rect,
            expanded: HashSet::new(),
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
                pressed: false,
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
                HOME_SIZE,
                HOME_SIZE,
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
            // 异形窗：窗口形状就是图标的双屏叠片轮廓（连阴影），轮廓外不画也不接点击
            unsafe {
                let _ = SetWindowRgn(hwnd, Some(home_region()), true);
            }
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

/// 悬浮键边长（物理像素）：比旧的 76 大一圈，手机上好点。
const HOME_SIZE: i32 = 104;
/// 图标的两块屏（后屏在左上、前屏在右下，互相叠着），与 App 图标同一构图。
fn plate_rects() -> (RECT, RECT) {
    (
        RECT { left: 16, top: 8, right: 68, bottom: 88 },
        RECT { left: 38, top: 18, right: 90, bottom: 98 },
    )
}

fn offset_rect(r: RECT, dx: i32, dy: i32) -> RECT {
    RECT { left: r.left + dx, top: r.top + dy, right: r.right + dx, bottom: r.bottom + dy }
}

/// 以中心为原点缩放（按压态把整个标记缩到 92%）。
fn scale_rect(r: RECT, milli: i32) -> RECT {
    let c = HOME_SIZE / 2;
    let f = |v: i32| c + (v - c) * milli / 1000;
    RECT { left: f(r.left), top: f(r.top), right: f(r.right), bottom: f(r.bottom) }
}

/// 悬浮键的窗口轮廓：两块屏 + 它们右下偏移的阴影，并集。
/// 每次调用新建一个 HRGN：给 SetWindowRgn 的由系统接管，给 PtInRegion 的用完自己删。
fn home_region() -> HRGN {
    let (back, front) = plate_rects();
    let parts = [back, front, offset_rect(back, 3, 5), offset_rect(front, 3, 5)];
    let mut acc = unsafe { CreateRoundRectRgn(parts[0].left, parts[0].top, parts[0].right, parts[0].bottom, 26, 26) };
    for p in &parts[1..] {
        let r = unsafe { CreateRoundRectRgn(p.left, p.top, p.right, p.bottom, 26, 26) };
        unsafe {
            CombineRgn(Some(acc), Some(acc), Some(r), RGN_OR);
            let _ = DeleteObject(HGDIOBJ(r.0));
        }
    }
    acc
}

type Rgb = (i32, i32, i32);

fn shade(c: Rgb, milli: i32) -> COLORREF {
    let f = |v: i32| ((v * milli / 1000).clamp(0, 255)) as u32;
    rgb(f(c.0), f(c.1), f(c.2))
}

fn lerp_rgb(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let f = |x: i32, y: i32| x + ((y - x) as f32 * t) as i32;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

/// 在圆角矩形内填竖向渐变：先把裁剪区设成这块屏的轮廓，再逐行刷。
fn draw_plate(hdc: HDC, rc: RECT, top: Rgb, bottom: Rgb, outline: Rgb, bright: i32) {
    unsafe {
        let clip = CreateRoundRectRgn(rc.left, rc.top, rc.right, rc.bottom, 26, 26);
        SelectClipRgn(hdc, Some(clip));
        let h = (rc.bottom - rc.top - 1).max(1) as f32;
        for y in rc.top..rc.bottom {
            let c = shade(lerp_rgb(top, bottom, (y - rc.top) as f32 / h), bright);
            let brush = CreateSolidBrush(c);
            let row = RECT { left: rc.left, top: y, right: rc.right, bottom: y + 1 };
            FillRect(hdc, &row, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
        }
        SelectClipRgn(hdc, None);
        let _ = DeleteObject(HGDIOBJ(clip.0));
        // 轮廓描边把两块屏分开，叠片的层次靠它
        let pen = CreatePen(PS_SOLID, 2, shade(outline, bright));
        let old_p = SelectObject(hdc, HGDIOBJ(pen.0));
        let null_brush = GetStockObject(NULL_BRUSH);
        let old_b = SelectObject(hdc, null_brush);
        let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, 26, 26);
        SelectObject(hdc, old_p);
        SelectObject(hdc, old_b);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

fn paint_home(hwnd: HWND) {
    let pressed = HOME.with(|c| c.borrow().as_ref().map(|s| s.pressed).unwrap_or(false));
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let scale = if pressed { 920 } else { 1000 };
        let bright = if pressed { 800 } else { 1000 };
        // 底铺阴影色：轮廓里没被屏盖住的部分就是阴影的月牙边
        let shadow = CreateSolidBrush(shade((101, 74, 18), bright));
        FillRect(hdc, &rc, shadow);
        let _ = DeleteObject(HGDIOBJ(shadow.0));
        let (back, front) = plate_rects();
        draw_plate(
            hdc,
            scale_rect(back, scale),
            (255, 224, 102),
            (255, 194, 26),
            (204, 150, 0),
            bright,
        );
        draw_plate(
            hdc,
            scale_rect(front, scale),
            (255, 176, 58),
            (255, 138, 0),
            (217, 119, 0),
            bright,
        );
        let _ = EndPaint(hwnd, &ps);
    }
}

fn set_home_pressed(hwnd: HWND, pressed: bool) {
    HOME.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.pressed = pressed;
        }
    });
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

unsafe extern "system" fn home_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_home(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            // 按下先给反馈（缩 + 暗），抬起才算数——和系统按钮一个手感
            set_home_pressed(hwnd, true);
            unsafe {
                let _ = SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let was_pressed = HOME.with(|c| c.borrow().as_ref().map(|s| s.pressed).unwrap_or(false));
            set_home_pressed(hwnd, false);
            unsafe {
                let _ = ReleaseCapture();
            }
            if was_pressed {
                // 抬起的位置还在图标轮廓内才打开（滑出去松手 = 取消）
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                let region = home_region();
                let inside = unsafe { PtInRegion(region, x, y) }.as_bool();
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(region.0));
                }
                if inside {
                    let rect = HOME.with(|c| c.borrow().as_ref().map(|s| s.rect));
                    if let Some(rect) = rect {
                        if let Err(e) = open(rect) {
                            crate::log::write(&format!("打开资源库失败：{e}"));
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            // 捕获被别人抢走时别把按压态挂死
            let pressed = HOME.with(|c| c.borrow().as_ref().map(|s| s.pressed).unwrap_or(false));
            if pressed {
                set_home_pressed(hwnd, false);
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
                let _ = c.borrow_mut().take();
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

/// 一个运行中窗口的 App 分组键：exe 路径优先（同名不同路径算两个 App），退回程序名。
fn app_key_of(w: &RunningWindow) -> String {
    if !w.exe_path.is_empty() {
        w.exe_path.clone()
    } else {
        w.app_name.clone()
    }
}

/// 当前分页 + 搜索过滤后的条目。
/// 「运行中」按 App 分组：一组一行，展开的组在行后跟它每个窗口的子行。
fn current_items(s: &LauncherState) -> Vec<Item> {
    match s.tab {
        Tab::Running => {
            let mut groups: Vec<(String, Vec<&RunningWindow>)> = Vec::new();
            for w in &s.running {
                let key = app_key_of(w);
                match groups.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, v)) => v.push(w),
                    None => groups.push((key, vec![w])),
                }
            }
            let mut out = Vec::new();
            for (key, wins) in groups {
                let first = wins[0];
                let name = if first.app_name.is_empty() {
                    first.title.clone()
                } else {
                    first.app_name.clone()
                };
                // 搜索：App 名或任一窗口标题命中都保留这组
                if !matches_query(&name, "", &s.query)
                    && !wins.iter().any(|w| matches_query("", &w.title, &s.query))
                {
                    continue;
                }
                let expanded = s.expanded.contains(&key);
                out.push(Item {
                    name: name.clone(),
                    sub: if wins.len() > 1 {
                        format!("{} 个窗口", wins.len())
                    } else {
                        first.title.clone()
                    },
                    icon_path: first.exe_path.clone(),
                    running: true,
                    action: Action::Focus(first.hwnd),
                    kind: RowKind::AppGroup,
                    close_hwnd: if wins.len() == 1 {
                        Some(first.hwnd)
                    } else {
                        None
                    },
                    app_key: key.clone(),
                    win_count: wins.len(),
                    expanded,
                    exe_path: first.exe_path.clone(),
                });
                if expanded {
                    for w in &wins {
                        out.push(Item {
                            name: w.title.clone(),
                            sub: String::new(),
                            icon_path: w.exe_path.clone(),
                            running: true,
                            action: Action::Focus(w.hwnd),
                            kind: RowKind::WindowRow,
                            close_hwnd: Some(w.hwnd),
                            app_key: key.clone(),
                            win_count: 1,
                            expanded: false,
                            exe_path: w.exe_path.clone(),
                        });
                    }
                }
            }
            out
        }
        Tab::All => {
            let to_item = |a: &InstalledApp, recent: bool| {
                let running = match_running(a, &s.running).is_some();
                Item {
                    name: a.name.clone(),
                    sub: if running {
                        "运行中".to_string()
                    } else if recent {
                        "最近使用".to_string()
                    } else {
                        String::new()
                    },
                    icon_path: a.icon_key(),
                    running,
                    action: Action::Launch(a.launch_spec()),
                    kind: RowKind::Installed,
                    close_hwnd: None,
                    app_key: String::new(),
                    win_count: 0,
                    expanded: false,
                    exe_path: a.exe_path.clone(),
                }
            };
            if !s.query.is_empty() {
                // 搜索时平铺全部（最近里的也都在全部名单内，不必分段）
                return s
                    .installed
                    .iter()
                    .filter(|a| matches_query(&a.name, "", &s.query))
                    .map(|a| to_item(a, false))
                    .collect();
            }
            // 无搜索：最近使用置顶，其余按名字序；在最近里出现过的不要在下面重复
            let recent_keys: HashSet<String> = s.recent.iter().map(|a| a.key()).collect();
            let mut out: Vec<Item> = s.recent.iter().map(|a| to_item(a, true)).collect();
            out.extend(
                s.installed
                    .iter()
                    .filter(|a| !recent_keys.contains(&a.key()))
                    .map(|a| to_item(a, false)),
            );
            out
        }
    }
}

/// 「全部」里一条 App 与运行中窗口的匹配：exe 路径精确匹配优先；
/// 没有 exe（打包/UWP 应用）才退回名字/标题启发式——UWP 窗口常挂在
/// ApplicationFrameHost 名下，只能靠标题认。
fn match_running<'a>(a: &InstalledApp, wins: &'a [RunningWindow]) -> Option<&'a RunningWindow> {
    if !a.exe_path.is_empty() {
        if let Some(w) = wins
            .iter()
            .find(|w| w.exe_path.eq_ignore_ascii_case(&a.exe_path))
        {
            return Some(w);
        }
    }
    wins.iter().find(|w| {
        !w.app_name.is_empty()
            && (a.name.eq_ignore_ascii_case(&w.app_name)
                || w.title.to_lowercase().contains(&a.name.to_lowercase()))
    })
}

/// 行尾 ✕ 按钮的矩形（绘制与命中共用一套几何）。
fn close_btn_rect(item: &Item, r: RECT, w: i32) -> Option<RECT> {
    if item.close_hwnd.is_none() {
        return None;
    }
    Some(RECT {
        left: w - 84,
        top: r.top + 16,
        right: w - 20,
        bottom: r.top + 72,
    })
}

/// 行尾展开钮的矩形（仅多窗口分组行）。
fn expand_chip_rect(item: &Item, r: RECT, w: i32) -> Option<RECT> {
    if item.kind != RowKind::AppGroup || item.win_count < 2 {
        return None;
    }
    Some(RECT {
        left: w - 176,
        top: r.top + 26,
        right: w - 20,
        bottom: r.top + 62,
    })
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

/// 请求关闭一个窗口（WM_CLOSE；应用可以不理会——关没关掉，刷新后的列表一目了然，不假报）。
fn close_window(hwnd: HWND, target: isize, title: &str) {
    unsafe {
        let _ = PostMessageW(
            Some(HWND(target as *mut core::ffi::c_void)),
            WM_CLOSE,
            WPARAM(0),
            LPARAM(0),
        );
    }
    crate::ui_log(&format!("资源库：已请求关闭窗口「{title}」"));
    pump(350);
    refresh_running(hwnd);
}

/// 重扫运行中窗口（关窗后列表立刻对上现实），清掉已消失 App 的展开状态并钳住滚动。
fn refresh_running(hwnd: HWND) {
    LAUNCHER.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.running = apps::running_windows();
            let keys: HashSet<String> = s.running.iter().map(app_key_of).collect();
            s.expanded.retain(|k| keys.contains(k));
            let items = current_items(s);
            let mut rc = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut rc);
            }
            let max_scroll = (content_height(s.tab, items.len(), rc.right - rc.left)
                - (rc.bottom - rc.top - CONTENT_TOP))
            .max(0);
            s.scroll = s.scroll.clamp(0, max_scroll);
        }
    });
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn run_action(action: Action, name: &str) {
    let rect = LAUNCHER.with(|c| c.borrow().as_ref().map(|s| s.rect));
    let Some(rect) = rect else { return };
    match action {
        Action::Focus(hwnd) => {
            if apps::fullscreen_on(hwnd, rect) {
                crate::ui_log(&format!("资源库：{name} 已全屏到虚拟屏"));
            } else {
                crate::ui_log(&format!(
                    "资源库：{name} 的窗口移动没到位（可能不听 SetWindowPos），把它现在的屏幕位置看一眼，日志发回主会话"
                ));
            }
            close();
        }
        Action::Launch(spec) => {
            // 这条 App 的目标 exe（从状态里按启动凭据反查）：运行中判定先按 exe 精确匹配
            let exe_path = LAUNCHER.with(|c| {
                c.borrow()
                    .as_ref()
                    .and_then(|s| {
                        s.installed
                            .iter()
                            .chain(s.recent.iter())
                            .find(|a| a.launch_spec() == spec)
                            .map(|a| a.exe_path.clone())
                    })
                    .unwrap_or_default()
            });
            // 已在运行的同一个 App：直接放它的窗口，不重复打开
            let existing = apps::running_windows().into_iter().find(|w| {
                (!exe_path.is_empty() && w.exe_path.eq_ignore_ascii_case(&exe_path))
                    || (!w.app_name.is_empty()
                        && (name.eq_ignore_ascii_case(&w.app_name)
                            || w.title.to_lowercase().contains(&name.to_lowercase())))
            });
            if let Some(w) = existing {
                if apps::fullscreen_on(w.hwnd, rect) {
                    crate::ui_log(&format!("资源库：{name} 已在运行，窗口已全屏到虚拟屏"));
                } else {
                    crate::ui_log(&format!(
                        "资源库：{name} 已在运行，但窗口移动没到位，日志发回主会话"
                    ));
                }
                close();
                return;
            }
            let before: HashSet<isize> = apps::running_windows().iter().map(|w| w.hwnd).collect();
            if !apps::launch_app(&spec) {
                crate::ui_log(&format!("资源库：打开 {name} 失败（系统拒绝启动）"));
                return;
            }
            crate::ui_log(&format!("资源库：正在打开 {name}，等它的窗口出现…"));
            close();
            // 等新窗口出现（最多 12 秒）：标题/App 名对得上的立即认；
            // 对不上的先攒着——第一个新窗口常是闪屏或加载窗，认最大的那个，
            // 且要稳定 1.5 秒（期间没再冒出新窗口）才落锤，到点有候选就用候选。
            let area_of = |h: isize| -> i64 {
                let mut r = RECT::default();
                unsafe {
                    let _ = GetWindowRect(HWND(h as *mut core::ffi::c_void), &mut r);
                }
                (r.right - r.left) as i64 * (r.bottom - r.top) as i64
            };
            let deadline = Instant::now() + Duration::from_secs(12);
            let mut found: Option<isize> = None;
            let mut candidate: Option<isize> = None;
            let mut candidate_since = Instant::now();
            while Instant::now() < deadline {
                pump(250);
                let now = apps::running_windows();
                let fresh: Vec<_> = now.iter().filter(|w| !before.contains(&w.hwnd)).collect();
                if let Some(w) = fresh.iter().find(|w| {
                    (!exe_path.is_empty() && w.exe_path.eq_ignore_ascii_case(&exe_path))
                        || w.title.to_lowercase().contains(&name.to_lowercase())
                        || name.eq_ignore_ascii_case(&w.app_name)
                }) {
                    found = Some(w.hwnd);
                    break;
                }
                let biggest = fresh.iter().max_by_key(|w| area_of(w.hwnd));
                match (biggest, candidate) {
                    (Some(b), Some(c)) if b.hwnd == c => {
                        if candidate_since.elapsed() >= Duration::from_millis(1500) {
                            found = Some(c);
                            break;
                        }
                    }
                    (Some(b), _) => {
                        candidate = Some(b.hwnd);
                        candidate_since = Instant::now();
                    }
                    _ => {}
                }
            }
            if found.is_none() {
                found = candidate;
            }
            match found {
                Some(hwnd) => {
                    if apps::fullscreen_on(hwnd, rect) {
                        crate::ui_log(&format!("资源库：{name} 的窗口已全屏到虚拟屏"));
                    } else {
                        crate::ui_log(&format!(
                            "资源库：{name} 的窗口出现了但移动没到位，日志发回主会话"
                        ));
                    }
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

/// 无边圆角填充（页眉分段控件的轨道与滑块用）。
fn fill_rounded(mem: HDC, rc: RECT, color: COLORREF, inset: i32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let ob = SelectObject(mem, HGDIOBJ(brush.0));
        let op = SelectObject(mem, GetStockObject(NULL_PEN));
        let _ = RoundRect(
            mem,
            rc.left + inset,
            rc.top + inset,
            rc.right - inset,
            rc.bottom - inset,
            20,
            20,
        );
        SelectObject(mem, op);
        SelectObject(mem, ob);
        let _ = DeleteObject(HGDIOBJ(brush.0));
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
            // 页眉按钮组：两页签做成胶囊分段控件（浅灰轨道 + 品牌金橙滑块），
            // 关闭做成珊瑚色圆钮配白 ✕，和运行圆点同一色系
            let track = RECT {
                left: run_rc.left,
                top: run_rc.top,
                right: all_rc.right,
                bottom: run_rc.bottom,
            };
            fill_rounded(mem, track, rgb(231, 231, 235), 0);
            for (rcx, label, active) in [
                (run_rc, "运行中", tab == Tab::Running),
                (all_rc, "全部", tab == Tab::All),
            ] {
                if active {
                    fill_rounded(mem, rcx, rgb(255, 172, 42), 3);
                }
                SetTextColor(
                    mem,
                    if active {
                        rgb(70, 44, 0)
                    } else {
                        rgb(108, 108, 116)
                    },
                );
                let mut trc = rcx;
                draw_text(mem, label, &mut trc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            }
            let d = close_rc.bottom - close_rc.top;
            let ccx = (close_rc.left + close_rc.right) / 2;
            let circle = RECT {
                left: ccx - d / 2,
                top: close_rc.top,
                right: ccx + d / 2,
                bottom: close_rc.bottom,
            };
            let brush = CreateSolidBrush(rgb(243, 107, 83));
            let ob = SelectObject(mem, HGDIOBJ(brush.0));
            let op = SelectObject(mem, GetStockObject(NULL_PEN));
            let _ = Ellipse(mem, circle.left, circle.top, circle.right, circle.bottom);
            SelectObject(mem, op);
            SelectObject(mem, ob);
            let _ = DeleteObject(HGDIOBJ(brush.0));
            SetTextColor(mem, rgb(255, 255, 255));
            let mut trc = circle;
            draw_text(mem, "✕", &mut trc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
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
                        let is_window_row = item.kind == RowKind::WindowRow;
                        // 文字右界给行尾按钮让位
                        let text_right = expand_chip_rect(item, r, w)
                            .map(|e| e.left - 12)
                            .or_else(|| close_btn_rect(item, r, w).map(|c| c.left - 12))
                            .unwrap_or(w - 20);
                        if let Some(hicon) = icon {
                            let (ix, iy, isz) = if is_window_row {
                                (56, r.top + 24, 40)
                            } else {
                                (20, r.top + 16, 56)
                            };
                            let _ = DrawIconEx(
                                mem,
                                ix,
                                iy,
                                HICON(hicon as *mut core::ffi::c_void),
                                isz,
                                isz,
                                0,
                                None,
                                DI_NORMAL,
                            );
                        }
                        SelectObject(mem, HGDIOBJ(font_name.0));
                        SetTextColor(mem, rgb(30, 30, 34));
                        let mut tr = RECT {
                            left: if is_window_row { 112 } else { 100 },
                            top: r.top + if is_window_row { 24 } else { 10 },
                            right: text_right,
                            bottom: r.top + if is_window_row { 64 } else { 50 },
                        };
                        draw_text(
                            mem,
                            &item.name,
                            &mut tr,
                            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                        );
                        if !is_window_row {
                            SelectObject(mem, HGDIOBJ(font_sub.0));
                            SetTextColor(mem, rgb(125, 125, 133));
                            let mut tr = RECT {
                                left: 100,
                                top: r.top + 50,
                                right: text_right,
                                bottom: r.top + 82,
                            };
                            draw_text(
                                mem,
                                &item.sub,
                                &mut tr,
                                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                            );
                        }
                        // 行尾按钮（展开钮 / 关闭钮）：浅灰圆角底 + 深色字
                        for (btn, label) in [
                            (
                                expand_chip_rect(item, r, w),
                                if item.expanded {
                                    "收起 ▴".to_string()
                                } else {
                                    format!("{} 窗 ▾", item.win_count)
                                },
                            ),
                            (close_btn_rect(item, r, w), "✕".to_string()),
                        ] {
                            if let Some(mut br) = btn {
                                let brush = CreateSolidBrush(rgb(228, 228, 233));
                                let old_brush = SelectObject(mem, HGDIOBJ(brush.0));
                                let old_pen = SelectObject(mem, GetStockObject(NULL_PEN));
                                let _ = RoundRect(mem, br.left, br.top, br.right, br.bottom, 12, 12);
                                SelectObject(mem, old_pen);
                                SelectObject(mem, old_brush);
                                let _ = DeleteObject(HGDIOBJ(brush.0));
                                SelectObject(mem, HGDIOBJ(font_name.0));
                                SetTextColor(mem, rgb(60, 60, 66));
                                draw_text(
                                    mem,
                                    &label,
                                    &mut br,
                                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                                );
                                SetTextColor(mem, rgb(30, 30, 34));
                            }
                        }
                    }
                    Tab::All => {
                        let tile_w = (w - 32) / cols_for(w);
                        let cx = r.left + tile_w / 2;
                        if let Some(hicon) = icon {
                            let _ = DrawIconEx(
                                mem,
                                cx - 44,
                                r.top + 16,
                                HICON(hicon as *mut core::ffi::c_void),
                                88,
                                88,
                                0,
                                None,
                                DI_NORMAL,
                            );
                        }
                        SelectObject(mem, HGDIOBJ(font_sub.0));
                        SetTextColor(mem, rgb(30, 30, 34));
                        let mut tr = RECT {
                            left: r.left + 6,
                            top: r.top + 114,
                            right: r.right - 6,
                            bottom: r.top + 162,
                        };
                        draw_text(mem, &item.name, &mut tr, DT_CENTER | DT_WORDBREAK);
                        if item.running {
                            let dot = CreateSolidBrush(rgb(243, 107, 83));
                            let old = SelectObject(mem, HGDIOBJ(dot.0));
                            let _ = Ellipse(mem, cx - 4, r.top + 168, cx + 4, r.top + 176);
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
            enum Hit {
                Run(Action, String),
                Toggle(String),
                Close(isize, String),
            }
            let hit = LAUNCHER.with(|c| {
                let cell = c.borrow();
                let s = cell.as_ref()?;
                let items = current_items(s);
                items.iter().enumerate().find_map(|(i, item)| {
                    let r = item_rect(s.tab, s.scroll, i, w);
                    if !(x >= r.left && x < r.right && y >= r.top && y < r.bottom) {
                        return None;
                    }
                    let inside =
                        |b: RECT| x >= b.left && x < b.right && y >= b.top && y < b.bottom;
                    if let (Some(cr), Some(t)) = (close_btn_rect(item, r, w), item.close_hwnd) {
                        if inside(cr) {
                            return Some(Hit::Close(t, item.name.clone()));
                        }
                    }
                    if let Some(er) = expand_chip_rect(item, r, w) {
                        if inside(er) {
                            return Some(Hit::Toggle(item.app_key.clone()));
                        }
                    }
                    Some(Hit::Run(item.action.clone(), item.name.clone()))
                })
            });
            match hit {
                Some(Hit::Run(action, name)) => run_action(action, &name),
                Some(Hit::Toggle(key)) => {
                    LAUNCHER.with(|c| {
                        if let Some(s) = c.borrow_mut().as_mut() {
                            if !s.expanded.remove(&key) {
                                s.expanded.insert(key);
                            }
                        }
                    });
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                Some(Hit::Close(target, title)) => close_window(hwnd, target, &title),
                None => {}
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
