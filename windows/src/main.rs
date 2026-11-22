//! Snug for Sunlogin — Windows 版 spike（Rust）
//!
//! 只验证一件事：向日葵手机端能不能用一块 Windows 虚拟屏（VDD），串流是否流畅。
//! 本工具做四件小事：枚举显示器并把 VDD 虚拟屏设成手机屏模式（尺寸按所选手机型号缩放，默认 1080×2346@60）；
//! 按需改 VDD 配置并启停驱动设备（可原样恢复）；在虚拟屏上开动/静对照的测试窗口；
//! 在虚拟屏上开 App 资源库（启动器）：运行中/全部两页 + 搜索，选中 App 自动全屏到虚拟屏。
//! 另盯向日葵的 desktopagent 进程：主控连入自动建屏、断开自动拆屏（可在界面关掉）。
//! 不做手势翻译（机主 2026-11-19 定）。

#![windows_subsystem = "windows"]

mod apps;
mod ccd;
mod devices;
mod display;
mod guide;
mod launcher;
mod log;
mod phones;
mod scale;
mod session;
mod settings;
mod testwin;
mod update;
mod vdd;

use display::DisplayInfo;
use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    FillRect, GetDC, GetDeviceCaps, InvalidateRect, RedrawWindow, ReleaseDC, RoundRect,
    SelectObject, SetBkMode, SetTextColor,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, HBRUSH, HDC, HFONT, HGDIOBJ, LOGPIXELSX,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE,
    SetBkColor, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, InitCommonControlsEx, EM_SCROLLCARET, EM_SETSEL, ICC_LISTVIEW_CLASSES,
    INITCOMMONCONTROLSEX,
    LVCOLUMNW, LVCF_SUBITEM, LVCF_TEXT, LVCF_WIDTH, LVIF_TEXT, LVITEMW, LVM_DELETEALLITEMS,
    LVM_INSERTCOLUMNW, LVM_INSERTITEMW, LVM_SETBKCOLOR, LVM_SETEXTENDEDLISTVIEWSTYLE,
    LVM_SETITEMTEXTW, LVM_SETTEXTBKCOLOR, LVM_SETTEXTCOLOR,
    LVS_EX_DOUBLEBUFFER, LVS_EX_FULLROWSELECT, LVS_REPORT, LVS_SHOWSELALWAYS, LVS_SINGLESEL,
};
use windows::Win32::UI::WindowsAndMessaging::*;

const ID_REFRESH: i32 = 1001;
const ID_CREATE: i32 = 1002;
const ID_REMOVE: i32 = 1003;
const ID_MODEL: i32 = 1007;
const ID_MODE_AUTO: i32 = 1008;
const ID_MODE_MANUAL: i32 = 1009;
/// 左上角图标菜单（系统菜单）里「高级模式」子菜单的命令 ID，走 WM_SYSCOMMAND。
/// 系统命令占 0xF000 段且低 4 位保留，自定义 ID 放低位、比较时按 & 0xFFF0 掩码。
const IDM_ADV_TEST: usize = 0x0110;
const IDM_ADV_LAUNCHER: usize = 0x0120;
const IDM_ADV_ONLY_VIRTUAL: usize = 0x0130;
/// 启动驱动检测的结果（wparam 1 = 驱动在，0 = 不在 → 直接开向导）
const WM_DRIVER_CHECKED: u32 = WM_APP + 30;

/// 带 --guide（或误写的 --guild）启动：直接打开配置向导，不等询问
static GUIDE_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// 组合框/按钮消息（用字面量，含义见 Win32 文档）
const CB_ADDSTRING: u32 = 0x0143;
const CB_GETCURSEL: u32 = 0x0147;
const CB_SETCURSEL: u32 = 0x014E;
const BM_SETCHECK: u32 = 0x00F1;

/// 状态行级别，决定状态文字颜色：就绪绿、待处理橙、一般情况默认色。
#[derive(Clone, Copy, PartialEq)]
enum StatusLevel {
    Ready,
    Warn,
    Neutral,
}

struct MainState {
    status: HWND,
    list: HWND,
    btn_refresh: HWND,
    btn_create: HWND,
    btn_remove: HWND,

    icon_box: HWND,
    detail: HWND,
    lbl_phone: HWND,
    /// 内容卡片的矩形（x, y, w, h），layout 时写入，WM_PAINT 据此画白卡片
    card_phone: (i32, i32, i32, i32),
    combo_model: HWND,
    radio_auto: HWND,
    radio_manual: HWND,
    model_label: HWND,
    phone_model_id: String,
    auto_create: bool,
    /// 高级模式：主控连入时仅输出虚拟屏（物理屏关掉，断开恢复）。
    only_virtual: bool,
    /// 物理屏当前是被本应用关掉的（与 vdd-state 的 physical_off 同步：
    /// 内存这份防状态文件被别的流程清掉后丢标记，文件那份管异常退出后的下次启动）。
    physical_off: bool,
    /// 「高级模式」子菜单句柄（更新勾选状态用）
    adv_menu: HMENU,
    /// 当前这块屏是自动建的（主控断开时只拆自动建的，手动建的不碰）。
    screen_auto_owned: bool,
    phone_hint: HWND,
    log_edit: HWND,
    log_text: String,
    displays: Vec<DisplayInfo>,
    dpi: i32,
    status_level: StatusLevel,
}

thread_local! {
    static MAIN: RefCell<Option<MainState>> = const { RefCell::new(None) };
}

// 单实例互斥量用的两个 kernel32 导入：windows 0.59 里 CreateMutexW 不在
// Win32::System::Threading 路径下（CI 实测 unresolved import），直接声明更稳。
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(
        attrs: *const std::ffi::c_void,
        initial_owner: i32,
        name: *const u16,
    ) -> *mut std::ffi::c_void;
    fn GetLastError() -> u32;
}

fn main() {
    log::init();
    if std::env::args().any(|a| a == "--guide" || a == "--guild") {
        GUIDE_REQUESTED.store(true, Ordering::Relaxed);
    }
    // GUI 子系统下 panic 默认无处可见，统一落到日志文件，方便事后排障
    std::panic::set_hook(Box::new(|info| {
        log::write(&format!("PANIC：{info}"));
    }));
    // 单实例：双开时两个进程会同时盯会话、抢着建/拆同一块虚拟屏
    // （2026-11-20 实测有用户同时开了两个）。句柄留到进程退出才释放。
    let mutex_name: Vec<u16> = "SnugSunloginWindows.SingleInstance\0"
        .encode_utf16()
        .collect();
    let _instance = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    if !_instance.is_null() && unsafe { GetLastError() } == 183 {
        // 183 = ERROR_ALREADY_EXISTS：已有实例在跑
        log::write("已有 SnugWindows 实例在运行，本实例退出");
        return;
    }
    log::write(&format!("SnugWindows v{} 启动", env!("SNUG_VERSION")));
    std::process::exit(run());
}

fn run() -> i32 {
    unsafe {
        // ListView 等公共控件要先登记（清单里已声明 Common-Controls 6.0）
        let icex = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES,
        };
        let _ = InitCommonControlsEx(&icex);
        let hinstance = match GetModuleHandleW(None) {
            Ok(m) => HINSTANCE(m.0),
            Err(_) => HINSTANCE::default(),
        };
        let class_w = display::wide("SnugSunloginMainWindow");
        // 窗口图标：build.rs 嵌进 exe 的资源 ID 1（Mac 版同款金橙标记）；取不到就用系统默认
        let app_icon = LoadIconW(Some(hinstance), PCWSTR(1 as *mut u16)).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(main_wnd_proc),
            hInstance: hinstance,
            hIcon: app_icon,
            lpszClassName: PCWSTR(class_w.as_ptr()),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: bg_brush(), // 现代主题底色 #F5F5F7（theme 区）
            ..Default::default()
        };
        RegisterClassW(&wc);
        // 初始窗口大小按桌面 DPI 缩放，高分屏上别缩成一小团
        let dpi0 = GetDeviceCaps(Some(GetDC(None)), LOGPIXELSX);
        let win_w = 700 * dpi0 / 96;
        let win_h = 760 * dpi0 / 96;
        let title_w = display::wide(&format!(
            "Snug for Sunlogin — Windows 版 v{}",
            env!("SNUG_VERSION")
        ));
        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            80,
            80,
            win_w,
            win_h,
            None,
            None,
            Some(hinstance),
            None,
        ) {
            Ok(h) => h,
            Err(_) => return 1,
        };
        // 小图标不在 WNDCLASSW 里（那是 WNDCLASSEXW 的字段），建完窗口用 WM_SETICON 补上
        if !app_icon.is_invalid() {
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(1)), // ICON_BIG
                Some(LPARAM(app_icon.0 as isize)),
            );
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(0)), // ICON_SMALL
                Some(LPARAM(app_icon.0 as isize)),
            );
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        0
    }
}

// MARK: UI 小工具

fn set_text(hwnd: HWND, s: &str) {
    let w = display::wide(s);
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(w.as_ptr()));
    }
}

pub(crate) fn mk_control(
    parent: HWND,
    class: &str,
    text: &str,
    style: WINDOW_STYLE,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
    font: HFONT,
) -> HWND {
    let class_w = display::wide(class);
    let text_w = display::wide(text);
    let hinstance = HINSTANCE(
        unsafe { GetModuleHandleW(None) }
            .map(|m| m.0)
            .unwrap_or_default(),
    );
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_w.as_ptr()),
            PCWSTR(text_w.as_ptr()),
            style,
            x,
            y,
            w,
            h,
            Some(parent),
            Some(HMENU(id as *mut core::ffi::c_void)),
            Some(hinstance),
            None,
        )
    }
    .unwrap_or_default();
    unsafe {
        SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
    }
    hwnd
}

// MARK: 现代主题（浅灰底 + 白卡片 + 品牌色按钮，替掉原生控件的系统灰）

pub(crate) fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | (g << 8) | (b << 16))
}

/// 窗口底色 #F5F5F7
pub(crate) fn theme_bg() -> COLORREF {
    rgb(245, 245, 247)
}

/// 卡片与输入区底色（纯白）
pub(crate) fn theme_card() -> COLORREF {
    rgb(255, 255, 255)
}

fn brush_of(color: COLORREF) -> HBRUSH {
    unsafe { CreateSolidBrush(color) }
}

/// 全局画刷只建一次（进程生命周期，不释放）：0 = 底色刷，1 = 白刷
fn theme_brush(which: usize) -> HBRUSH {
    static BRUSHES: std::sync::OnceLock<[usize; 2]> = std::sync::OnceLock::new();
    let arr = BRUSHES.get_or_init(|| {
        [
            brush_of(theme_bg()).0 as usize,
            brush_of(theme_card()).0 as usize,
        ]
    });
    HBRUSH(arr[which] as *mut core::ffi::c_void)
}

pub(crate) fn bg_brush() -> HBRUSH {
    theme_brush(0)
}

pub(crate) fn card_brush() -> HBRUSH {
    theme_brush(1)
}

/// 自绘按钮（主窗口与向导共用）：圆角、无系统灰。
/// 主按钮（创建 / 向导的动作钮）品牌金橙，删除钮珊瑚红，其余白底描边。
/// 闪烁相位走按钮的 GWLP_USERDATA（向导定时器写 0/1），相位 1 时主按钮描深色粗边。
pub(crate) fn draw_button(dis: &DRAWITEMSTRUCT) {
    let id = dis.CtlID as i32;
    let state = dis.itemState.0;
    let pressed = state & 0x0001 != 0; // ODS_SELECTED
    let disabled = state & 0x0004 != 0; // ODS_DISABLED
    let flash_on = unsafe { GetWindowLongW(dis.hwndItem, GWLP_USERDATA) } == 1;
    let (fill, border, border_w, txt) = if disabled {
        (rgb(240, 240, 242), rgb(230, 230, 233), 1, rgb(168, 168, 173))
    } else if id == ID_CREATE || id == 3002 {
        // 主按钮：品牌金橙 + 深棕字（与启动器选中态、应用图标同色）
        let fill = if pressed || flash_on {
            rgb(238, 156, 20)
        } else {
            rgb(255, 172, 42)
        };
        (fill, fill, if flash_on { 3 } else { 1 }, rgb(70, 44, 0))
    } else if id == ID_REMOVE {
        let fill = if pressed {
            rgb(226, 90, 66)
        } else {
            rgb(243, 107, 83)
        };
        (fill, fill, 1, rgb(255, 255, 255))
    } else {
        let fill = if pressed {
            rgb(240, 240, 244)
        } else {
            rgb(255, 255, 255)
        };
        (fill, rgb(209, 209, 214), 1, rgb(29, 29, 31))
    };
    let hdc = dis.hDC;
    let rc = dis.rcItem;
    unsafe {
        let brush = CreateSolidBrush(fill);
        let pen = CreatePen(PS_SOLID, border_w, border);
        let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, 12, 12);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = DeleteObject(HGDIOBJ(pen.0));
        // 文字：用按钮自己的字体，居中绘制
        let font = HFONT(
            SendMessageW(dis.hwndItem, WM_GETFONT, Some(WPARAM(0)), Some(LPARAM(0))).0
                as *mut core::ffi::c_void,
        );
        let old_font = SelectObject(hdc, HGDIOBJ(font.0));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, txt);
        let mut buf = [0u16; 128];
        let n = GetWindowTextW(dis.hwndItem, &mut buf);
        if n > 0 {
            let mut rect = rc;
            let _ = DrawTextW(
                hdc,
                &mut buf[..n as usize],
                &mut rect as *mut _,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        SelectObject(hdc, old_font);
    }
}

/// 建一个 UI 字体：高度按磅数 × DPI 换算（负数表示按字符高度匹配）。
/// 注意 windows 0.59 里 CreateFontW 的 weight 是裸 i32、其余是类型化常量，别混。
fn make_font(face: &str, points: i32, weight: i32, dpi: i32) -> HFONT {
    let face_w = display::wide(face);
    unsafe {
        CreateFontW(
            -points * dpi / 72,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            PCWSTR(face_w.as_ptr()),
        )
    }
}

/// 把控件挪到指定位置尺寸（布局统一走这里，坐标都是物理像素）。
fn move_ctrl(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    if hwnd.is_invalid() || w <= 0 || h <= 0 {
        return;
    }
    unsafe {
        let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER);
    }
}

/// 按当前客户区大小排一遍控件，构图学 Mac 版设置页：
/// 顶部状态区（图标 + 标题 + 说明）→ 显示器列表 → 「手机屏」卡片 → 日志吃掉剩余高度。
/// （测试入口已挪进左上角图标菜单的「高级模式」，日志因此更高。）
/// 坐标先按 96 DPI 的设计值写，再乘窗口 DPI 缩放，高分屏上比例不变。
fn layout_controls(client_w: i32, client_h: i32) {
    let items = MAIN.with(|c| {
        let cell = c.borrow();
        cell.as_ref().map(|s| {
            (
                s.icon_box,
                s.status,
                s.detail,
                s.list,
                s.lbl_phone,
                s.btn_create,
                s.btn_remove,
                s.model_label,
                s.combo_model,
                s.radio_auto,
                s.radio_manual,
                s.phone_hint,
                s.btn_refresh,
                s.log_edit,
                s.dpi,
                s.auto_create,
            )
        })
    });
    let Some((
        icon_box,
        status,
        detail,
        list,
        lbl_phone,
        b_create,
        b_remove,
        model_label,
        combo_model,
        radio_auto,
        radio_manual,
        phone_hint,
        b_refresh,
        log_edit,
        dpi,
        auto_create,
    )) = items
    else {
        return;
    };
    let sc = |v: i32| v * dpi / 96;
    let m = sc(16);
    let cw = client_w - m * 2;
    // 状态区：图标在左，标题与说明两行在右
    move_ctrl(icon_box, m, sc(12), sc(40), sc(40));
    move_ctrl(status, m + sc(50), sc(10), cw - sc(50), sc(24));
    move_ctrl(detail, m + sc(50), sc(36), cw - sc(50), sc(20));
    // 显示器列表
    move_ctrl(list, m, sc(64), cw, sc(116));
    // 列表正下方的操作行：刷新 / 创建 / 删除，只在手动模式出现；
    // 自动模式下建屏拆屏由主控连入/断开触发，这行整个让掉不占地方
    let list_bottom = sc(64) + sc(116);
    let row_h = if auto_create { 0 } else { sc(46) };
    if auto_create {
        for b in [b_refresh, b_create, b_remove] {
            unsafe {
                let _ = ShowWindow(b, SW_HIDE);
            }
        }
    } else {
        let bw3 = (cw - sc(16)) / 3;
        let by = list_bottom + sc(10);
        move_ctrl(b_refresh, m, by, bw3, sc(36));
        move_ctrl(b_create, m + bw3 + sc(8), by, bw3, sc(36));
        move_ctrl(
            b_remove,
            m + (bw3 + sc(8)) * 2,
            by,
            cw - (bw3 + sc(8)) * 2,
            sc(36),
        );
        for b in [b_refresh, b_create, b_remove] {
            unsafe {
                let _ = ShowWindow(b, SW_SHOW);
            }
        }
    }
    // 「手机屏」卡片：组名标签 + 型号下拉、自动/手动开关、说明（卡片本体在 WM_PAINT 画）
    let gy = list_bottom + row_h + sc(12);
    let gh_phone = sc(126);
    move_ctrl(lbl_phone, m + sc(14), gy + sc(10), sc(200), sc(20));
    let inner = cw - sc(28);
    move_ctrl(model_label, m + sc(14), gy + sc(42), sc(56), sc(20));
    // 下拉框的矩形高含展开后的列表部分，收起时只占一行
    move_ctrl(
        combo_model,
        m + sc(14) + sc(60),
        gy + sc(39),
        inner - sc(60),
        sc(180),
    );
    let radio_w = inner * 58 / 100;
    move_ctrl(radio_auto, m + sc(14), gy + sc(74), radio_w, sc(22));
    move_ctrl(
        radio_manual,
        m + sc(14) + radio_w + sc(8),
        gy + sc(74),
        inner - radio_w - sc(8),
        sc(22),
    );
    move_ctrl(phone_hint, m + sc(14), gy + sc(102), inner, sc(18));
    // 卡片矩形存回状态，WM_PAINT 照此画白卡片
    MAIN.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.card_phone = (m, gy, cw, gh_phone);
        }
    });
    // 日志吃掉剩余高度（测试卡片挪进菜单后，这里比原来高出一整张卡）
    let log_y = gy + gh_phone + sc(12);
    move_ctrl(log_edit, m, log_y, cw, client_h - log_y - m);
}

/// 给显示器列表（ListView 报表模式）建列，列宽按 DPI 缩放。
fn init_list_columns(list: HWND, dpi: i32) {
    let cols: [(&str, i32); 6] = [
        ("显示器", 105),
        ("适配器", 175),
        ("分辨率", 90),
        ("刷新率", 62),
        ("位置", 78),
        ("备注", 150),
    ];
    for (i, (title, w)) in cols.iter().enumerate() {
        let mut buf = display::wide(title);
        let col = LVCOLUMNW {
            mask: LVCF_WIDTH | LVCF_TEXT | LVCF_SUBITEM,
            cx: w * dpi / 96,
            pszText: PWSTR(buf.as_mut_ptr()),
            iSubItem: i as i32,
            ..Default::default()
        };
        unsafe {
            SendMessageW(
                list,
                LVM_INSERTCOLUMNW,
                Some(WPARAM(i)),
                Some(LPARAM(&col as *const LVCOLUMNW as isize)),
            );
        }
    }
    let ex = (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize;
    unsafe {
        SendMessageW(
            list,
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            Some(WPARAM(ex as usize)),
            Some(LPARAM(ex)),
        );
    }
}

/// 把显示器枚举填进 ListView：一行一块屏，分列显示。
fn fill_display_list(list: HWND, displays: &[DisplayInfo]) {
    unsafe {
        SendMessageW(list, LVM_DELETEALLITEMS, Some(WPARAM(0)), Some(LPARAM(0)));
    }
    for (i, d) in displays.iter().enumerate() {
        let mut tags: Vec<String> = Vec::new();
        if d.primary {
            tags.push("主屏".to_string());
        }
        if d.is_virtual() {
            tags.push("VDD 虚拟屏".to_string());
        }
        if d.is_phone_mode() {
            tags.push("手机屏模式".to_string());
        }
        if !d.monitor_name.is_empty() && d.monitor_name != d.adapter_name {
            tags.push(d.monitor_name.clone());
        }
        let cells = [
            d.device_name.clone(),
            d.adapter_name.clone(),
            format!("{}×{}", d.width, d.height),
            format!("{} Hz", d.hz),
            format!("{},{}", d.x, d.y),
            tags.join(" · "),
        ];
        for (col, text) in cells.iter().enumerate() {
            let mut buf = display::wide(text);
            let item = LVITEMW {
                mask: LVIF_TEXT,
                iItem: i as i32,
                iSubItem: col as i32,
                pszText: PWSTR(buf.as_mut_ptr()),
                ..Default::default()
            };
            unsafe {
                if col == 0 {
                    SendMessageW(
                        list,
                        LVM_INSERTITEMW,
                        Some(WPARAM(0)),
                        Some(LPARAM(&item as *const LVITEMW as isize)),
                    );
                } else {
                    SendMessageW(
                        list,
                        LVM_SETITEMTEXTW,
                        Some(WPARAM(i)),
                        Some(LPARAM(&item as *const LVITEMW as isize)),
                    );
                }
            }
        }
    }
}

/// 写一行日志：落盘 + 显示到界面的日志框。（pub(crate)：启动器也要往这里写）
pub(crate) fn ui_log(msg: &str) {
    let line = log::write(msg);
    let (edit, text) = MAIN.with(|c| {
        let mut cell = c.borrow_mut();
        match cell.as_mut() {
            Some(s) => {
                if !s.log_text.is_empty() {
                    s.log_text.push_str("\r\n");
                }
                s.log_text.push_str(&line);
                if s.log_text.len() > 24000 {
                    let cut = s.log_text.len() - 16000;
                    let drain_to = s.log_text[cut..]
                        .find('\n')
                        .map(|i| cut + i + 1)
                        .unwrap_or(cut);
                    s.log_text.drain(..drain_to);
                }
                (s.log_edit, s.log_text.clone())
            }
            None => (HWND::default(), String::new()),
        }
    });
    if !edit.is_invalid() {
        set_text(edit, &text);
        // 光标挪到文末并滚过去，新日志一进来就看得见，不用手动往下拖
        let len = text.encode_utf16().count();
        unsafe {
            SendMessageW(edit, EM_SETSEL, Some(WPARAM(len)), Some(LPARAM(len as isize)));
            SendMessageW(edit, EM_SCROLLCARET, Some(WPARAM(0)), Some(LPARAM(0)));
        }
    }
}

/// 等待 ms 毫秒但继续泵消息，界面不假死。
fn wait_pump(ms: u64) {
    let start = Instant::now();
    let mut msg = MSG::default();
    while start.elapsed() < Duration::from_millis(ms) {
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

// MARK: 业务动作

/// 状态文案学 Mac 版：一句标题 + 一句短说明，颜色按级别（就绪绿、待处理橙、一般灰）。
fn status_info(
    displays: &[DisplayInfo],
    auto_create: bool,
    agent_present: bool,
) -> (String, String, StatusLevel) {
    if let Some(d) = displays.iter().find(|d| d.is_virtual() && d.is_phone_mode()) {
        return (
            "手机屏已就绪".to_string(),
            format!("{}×{}@{} · 向日葵手机端选这块屏", d.width, d.height, d.hz),
            StatusLevel::Ready,
        );
    }
    if let Some(d) = displays.iter().find(|d| d.is_virtual()) {
        return (
            "虚拟屏已创建".to_string(),
            format!(
                "当前 {}×{}@{}，点「创建手机屏」设成手机尺寸",
                d.width, d.height, d.hz
            ),
            StatusLevel::Warn,
        );
    }
    if auto_create && !agent_present {
        let (w, h, _) = phones::current_spec();
        return (
            "等待主控连入".to_string(),
            format!("向日葵主控连入后自动建屏 · 当前型号 {w}×{h}"),
            StatusLevel::Neutral,
        );
    }
    if vdd::settings_exists() {
        return (
            "虚拟屏未启动".to_string(),
            "驱动可能没装好或被禁用，打开配置向导排查".to_string(),
            StatusLevel::Warn,
        );
    }
    (
        "未检测到虚拟屏驱动".to_string(),
        "先打开配置向导装驱动，再创建手机屏".to_string(),
        StatusLevel::Neutral,
    )
}

fn refresh_displays() {
    let displays = display::list();
    let auto_create = MAIN.with(|c| {
        c.borrow()
            .as_ref()
            .map(|s| s.auto_create)
            .unwrap_or(true)
    });
    let (headline, detail_text, level) = status_info(
        &displays,
        auto_create,
        session::AGENT_PRESENT.load(Ordering::Relaxed),
    );
    let (list, status, detail) = MAIN.with(|c| {
        let mut cell = c.borrow_mut();
        match cell.as_mut() {
            Some(s) => {
                s.displays = displays.clone();
                s.status_level = level;
                (s.list, s.status, s.detail)
            }
            None => (HWND::default(), HWND::default(), HWND::default()),
        }
    });
    if !list.is_invalid() {
        fill_display_list(list, &displays);
    }
    if !status.is_invalid() {
        set_text(status, &headline);
    }
    if !detail.is_invalid() {
        set_text(detail, &detail_text);
    }
    ui_log(&format!("显示器枚举：{} 块", displays.len()));
    for d in &displays {
        ui_log(&format!("  {}", d.describe()));
    }
    ui_log(&format!("状态：{headline}（{detail_text}）"));
}

/// 重启 VDD 设备（找设备 → pnputil 重启）。返回是否找到了设备。
/// 找不到 VDD 适配器时把现场打全，让下一次贴回来的日志能直接定性：
/// 是驱动根本没装、装了但设备名不一样、还是包已装但设备没建出来。
fn log_no_vdd_diagnostics(list: &[devices::PnpDevice]) {
    ui_log(&format!("诊断：PnP 显示类设备共 {} 个：", list.len()));
    for d in list {
        ui_log(&format!(
            "  {} | {} | 状态 {}",
            d.friendly_name, d.instance_id, d.status
        ));
    }
    match std::fs::read_dir("C:\\VirtualDisplayDriver") {
        Ok(rd) => {
            let names: Vec<String> = rd
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            ui_log(&format!(
                "诊断：C:\\VirtualDisplayDriver 目录内容：{}",
                if names.is_empty() {
                    "（空）".to_string()
                } else {
                    names.join("、")
                }
            ));
        }
        Err(e) => ui_log(&format!("诊断：读不了 C:\\VirtualDisplayDriver 目录：{e}")),
    }
    match devices::problem_devices() {
        Ok(problems) => {
            ui_log(&format!("诊断：状态异常的 PnP 设备共 {} 个：", problems.len()));
            for d in problems.iter().take(10) {
                ui_log(&format!(
                    "  {} | {} | 状态 {}",
                    d.friendly_name, d.instance_id, d.status
                ));
            }
        }
        Err(e) => ui_log(&format!("诊断：{e}")),
    }
    ui_log("诊断：包在、节点不在，或设备在但驱动没真正起来（管道不通）这类问题，跑同目录的 setup-virtual-display.ps1 排查，它会把驱动活性与系统事件一并查清");
}

fn restart_vdd_device() -> bool {
    match devices::display_adapters() {
        Ok(list) => match devices::find_vdd(&list) {
            Some(dev) => {
                ui_log(&format!(
                    "重启 VDD 设备让配置生效：{}（{}，状态 {}）",
                    dev.friendly_name, dev.instance_id, dev.status
                ));
                let (exit, output) = devices::restart_device(&dev.instance_id);
                ui_log(&format!("pnputil 退出码 {exit}"));
                if exit != 0 {
                    let tail: Vec<&str> = output.lines().rev().take(3).collect();
                    for line in tail.iter().rev() {
                        ui_log(&format!("  {line}"));
                    }
                }
                true
            }
            None => {
                ui_log("没找到 VDD 显示适配器（驱动没装？）——配置已写好，装好驱动后重启设备即可生效");
                log_no_vdd_diagnostics(&list);
                false
            }
        },
        Err(e) => {
            ui_log(&e);
            false
        }
    }
}

/// 轮询等虚拟屏挂上桌面（带消息泵，UI 不冻）。出现返回 true，超时返回 false。
fn poll_virtual(secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if display::list().iter().any(|d| d.is_virtual()) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        wait_pump(500);
    }
}

/// 对 VDD 设备做一次禁用→启用完整循环并记日志（建屏阶梯的最后一级）。
fn cycle_vdd_device() {
    match devices::display_adapters() {
        Ok(list) => match devices::find_vdd(&list) {
            Some(dev) => {
                ui_log(&format!(
                    "禁用→启用循环 VDD 设备：{}（{}，状态 {}）",
                    dev.friendly_name, dev.instance_id, dev.status
                ));
                let (exit, _) = devices::cycle_device(&dev.instance_id);
                ui_log(&format!("循环后启用退出码 {exit}"));
            }
            None => ui_log("禁用→启用循环：没找到 VDD 设备，跳过"),
        },
        Err(e) => ui_log(&e),
    }
}

/// 拓扑层激活 VDD 路径（ccd 模块）：过程说明逐条记日志；
/// 返回 true 表示真的应用了激活（调用方再轮询枚举确认效果）。
fn try_activate_vdd_path() -> bool {
    match ccd::activate_vdd() {
        Ok(outcome) => {
            for note in &outcome.notes {
                ui_log(&format!("拓扑：{note}"));
            }
            if outcome.applied {
                ui_log("拓扑：已把 VDD 路径激活并存盘");
            } else if outcome.found && outcome.was_active {
                ui_log("拓扑：VDD 路径本就是激活的，问题不在这一层");
            }
            outcome.applied
        }
        Err(e) => {
            ui_log(&format!("拓扑：{e}"));
            false
        }
    }
}

/// 对 VDD 设备做一个 pnputil 动作（启用/禁用）并记日志。没找到设备或枚举失败返回 false。
fn vdd_device_action(verb: &str, action: fn(&str) -> (i32, String)) -> bool {
    match devices::display_adapters() {
        Ok(list) => match devices::find_vdd(&list) {
            Some(dev) => {
                ui_log(&format!(
                    "{verb} VDD 设备：{}（{}，状态 {}）",
                    dev.friendly_name, dev.instance_id, dev.status
                ));
                let (exit, output) = action(&dev.instance_id);
                ui_log(&format!("pnputil 退出码 {exit}"));
                if exit != 0 {
                    let tail: Vec<&str> = output.lines().rev().take(3).collect();
                    for line in tail.iter().rev() {
                        ui_log(&format!("  {line}"));
                    }
                }
                exit == 0
            }
            None => {
                ui_log("没找到 MttVDD 设备（硬件 ID ROOT\\MttVDD）——驱动没装好，先运行安装脚本 setup-virtual-display.ps1");
                false
            }
        },
        Err(e) => {
            ui_log(&e);
            false
        }
    }
}

pub(crate) fn create_phone_screen() {
    ui_log("—— 创建手机屏 ——");
    let (pw, ph, phz) = phones::current_spec();
    let mut state = vdd::load_state();
    let (mut text, created) = match vdd::load_settings() {
        Ok(t) => (t, false),
        Err(_) => {
            ui_log("配置文件不存在，用官方模板新建（已含手机屏分辨率与 count=1）");
            (vdd::create_default_settings(), true)
        }
    };
    state.created_file = created;
    if state.original_count.is_none() && !created {
        state.original_count = vdd::get_count(&text);
    }
    if !created {
        let count = vdd::get_count(&text).unwrap_or(0);
        if count < 1 {
            text = vdd::set_count(&text, 1);
            ui_log("monitors/count 设为 1");
        }
        if vdd::has_phone_resolution(&text) {
            ui_log(&format!("配置里已有 {pw}×{ph} 分辨率，不重复加"));
        } else {
            text = vdd::add_phone_resolution(&text);
            state.added_resolution = true;
            ui_log(&format!("已往配置加入 {pw}×{ph}@{phz} 分辨率"));
        }
    }
    if let Err(e) = vdd::save_settings(&text) {
        ui_log(&format!("写配置文件失败：{e}"));
        return;
    }
    vdd::save_state(&state);
    if created {
        ui_log("配置已写入 C:\\VirtualDisplayDriver\\vdd_settings.xml（新建文件，无原文件可备份）");
    } else {
        ui_log("配置已写入 C:\\VirtualDisplayDriver\\vdd_settings.xml（原文件备份为 .snug-pristine.bak）");
    }

    // 移除手机屏会把设备禁用：先启用回来（本就启用时重复启用无害，成败只记日志，
    // 后面的重启设备自带 disable+enable 兜底，不在这里中断）
    vdd_device_action("启用", devices::enable_device);
    if !restart_vdd_device() {
        return;
    }
    // 建屏阶梯：一步没把虚拟屏带起来就上下一步，每步都轮询等它挂上桌面
    // （真机实测约 2 秒，慢的机器更久，别用一次固定等待赌时间）。
    // 第一级重启设备是老路；2026-11-21 真机定案它失效的原因：向日葵摘屏后
    // 虚拟屏的显示路径在持久化拓扑里未激活，设备重启退出码 0、设备 Started、
    // 管道在、配置对，显示就是不回来——第二级拓扑激活专治这一层；
    // 后面两级是设备层后路：nefcon 按硬件 ID 重装节点、禁用→启用完整循环。
    let mut up = poll_virtual(10);
    if !up {
        // 第二级：拓扑激活——2026-11-21 定案的主要失效模式。向日葵断开会把
        // 虚拟屏路径摘出活动拓扑并存盘，设备/驱动/配置全正常它也不回来，
        // 只有把 CCD 路径重新激活才行（ccd.rs）。
        ui_log("重启设备后虚拟屏没回来，检查显示拓扑…");
        if try_activate_vdd_path() {
            up = poll_virtual(10);
        }
    }
    if !up {
        ui_log("拓扑这级没解决，改走 nefcon 重装 VDD 节点…");
        match devices::nefcon_install() {
            Some((exit, output)) => {
                ui_log(&format!(
                    "nefcon install 退出码 {exit}（0/3010 均算装上，最终看枚举）"
                ));
                if exit != 0 && exit != 3010 {
                    let tail: Vec<&str> = output.lines().rev().take(3).collect();
                    for line in tail.iter().rev() {
                        ui_log(&format!("  {line}"));
                    }
                }
            }
            None => {
                ui_log("随包里没找到 nefcon 或 MttVDD.inf，跳过 nefcon 这一步");
            }
        }
        if !poll_virtual(15) {
            ui_log("nefcon 后仍没虚拟屏，再试禁用→启用循环…");
            cycle_vdd_device();
            // 设备重现后路径可能仍未激活，再补一次拓扑激活
            if !poll_virtual(5) && try_activate_vdd_path() {
                poll_virtual(10);
            }
        }
    }
    refresh_displays();

    // 重启后虚拟屏可能还停在旧模式，手动设成手机屏模式
    let virtual_now = MAIN.with(|c| {
        c.borrow()
            .as_ref()
            .and_then(|s| s.displays.iter().find(|d| d.is_virtual()).cloned())
    });
    match virtual_now {
        Some(d) if d.is_phone_mode() => {
            ui_log(&format!("完成：手机屏已就绪（{pw}×{ph}@{phz}）。用向日葵手机端连这台电脑、选这块屏测试"));
            apply_phone_scale();
        }
        Some(d) => {
            ui_log(&format!(
                "虚拟屏当前 {}×{}@{}，设置成手机屏模式…",
                d.width, d.height, d.hz
            ));
            let r = display::set_mode(&d.device_name, pw, ph, phz);
            ui_log(&format!(
                "ChangeDisplaySettingsEx 返回 {r}（0 = 成功）"
            ));
            wait_pump(900);
            refresh_displays();
            let ok = MAIN.with(|c| {
                c.borrow()
                    .as_ref()
                    .map(|s| s.displays.iter().any(|d| d.is_virtual() && d.is_phone_mode()))
                    .unwrap_or(false)
            });
            if ok {
                ui_log(&format!("完成：手机屏已就绪（{pw}×{ph}@{phz}）。用向日葵手机端连这台电脑、选这块屏测试"));
                apply_phone_scale();
            } else {
                ui_log(&format!("设模式后仍不是 {pw}×{ph}，看上面枚举结果确认虚拟屏实际模式，并把日志贴回主会话"));
            }
        }
        None => {
            ui_log("建屏阶梯（重启设备／拓扑激活／nefcon 重装／禁用启用循环）都没把虚拟屏带起来：打开设备管理器看 Virtual Display Driver 的状态，把这段日志贴回主会话");
        }
    }
}

/// 手机屏就绪后把它的显示缩放抬到至少 250%（手机上界面更大、更好点）；
/// 只升不降；第一次改动前把原始相对步数记进状态文件，移除手机屏时还原。
fn apply_phone_scale() {
    let Some(v) = display::list().into_iter().find(|d| d.is_virtual()) else {
        return;
    };
    let info = match scale::query(&v.device_name) {
        Ok(i) => i,
        Err(e) => {
            ui_log(&format!("虚拟屏缩放：{e}"));
            return;
        }
    };
    if info.percent >= scale::PHONE_MIN_PERCENT {
        ui_log(&format!("虚拟屏缩放已是 {}%，够大，不动", info.percent));
        return;
    }
    let mut state = vdd::load_state();
    if state.scale_rel_before.is_none() {
        state.scale_rel_before = Some(info.rel);
        vdd::save_state(&state);
    }
    match scale::set_percent(&v.device_name, scale::PHONE_MIN_PERCENT) {
        Ok(actual) => ui_log(&format!(
            "虚拟屏缩放 {}% → {actual}%（手机上界面更大、更好点）",
            info.percent
        )),
        Err(e) => ui_log(&format!("虚拟屏缩放没调成：{e}")),
    }
}

pub(crate) fn remove_phone_screen() {
    ui_log("—— 移除手机屏（恢复配置） ——");
    let state = vdd::load_state();
    // 缩放还原要趁虚拟屏还在时做：后面设备一禁用就够不着这块屏了
    if let Some(rel) = state.scale_rel_before {
        match display::list().into_iter().find(|d| d.is_virtual()) {
            Some(v) => match scale::set_rel(&v.device_name, rel) {
                Ok(()) => ui_log("虚拟屏缩放已还原到建屏前"),
                Err(e) => ui_log(&format!("虚拟屏缩放还原失败：{e}")),
            },
            None => ui_log("虚拟屏当前不在，缩放没法还原（它下次出现时可能仍是放大值）"),
        }
    }
    if state.created_file && vdd::settings_exists() {
        // 配置文件是本应用新建的：整个删掉就是最干净的回滚
        match std::fs::remove_file(vdd::SETTINGS_PATH) {
            Ok(()) => ui_log("已删除本应用新建的配置文件"),
            Err(e) => ui_log(&format!("删配置文件失败：{e}")),
        }
    } else if vdd::settings_exists() {
        match vdd::load_settings() {
            Ok(text) => {
                // 先把配置现状打进日志：解析口径和下面的手术一致，删没删掉当场对得上账
                ui_log(&format!(
                    "配置现状：count={}，分辨率：{}",
                    vdd::get_count(&text)
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "读不到".to_string()),
                    vdd::resolution_summary(&text)
                ));
                let (mut text, removed) = vdd::remove_phone_resolution(&text);
                if removed > 0 {
                    ui_log(&format!("已从配置删掉 {removed} 条手机屏分辨率"));
                } else {
                    ui_log("配置里没有手机屏分辨率，无需删");
                }
                if let Some(orig) = state.original_count {
                    text = vdd::set_count(&text, orig);
                    ui_log(&format!("monitors/count 恢复为 {orig}"));
                }
                if let Err(e) = vdd::save_settings(&text) {
                    ui_log(&format!("写配置文件失败：{e}"));
                    return;
                }
            }
            Err(e) => {
                ui_log(&format!("读配置文件失败：{e}"));
                return;
            }
        }
    } else {
        ui_log("没有配置文件，无需恢复");
    }
    vdd::clear_state();
    // 配置改回去还不够：MttVDD 只要设备启用着就一直挂着这块屏（count 恢复成 1 也还有一块），
    // 必须禁用设备它才真的消失；下次创建会先启用回来。
    if vdd_device_action("禁用", devices::disable_device) {
        wait_pump(1500);
    }
    refresh_displays();
    ui_log("（原始配置的完整备份仍在 vdd_settings.xml.snug-pristine.bak）");
    // 以枚举结果为准下结论，不无条件报「移除完成」
    let still_there = MAIN.with(|c| {
        c.borrow()
            .as_ref()
            .map(|s| s.displays.iter().any(|d| d.is_virtual()))
            .unwrap_or(false)
    });
    if still_there {
        ui_log("⚠ 移除后虚拟屏还在（禁用没生效？）——把这段日志贴回主会话");
    } else {
        ui_log("移除完成：虚拟屏已消失（下次点「创建手机屏」会自动重新启用驱动）");
    }
}

fn set_screen_owner(auto: bool) {
    MAIN.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.screen_auto_owned = auto;
        }
    });
}

fn persist_settings() {
    MAIN.with(|c| {
        if let Some(s) = c.borrow().as_ref() {
            settings::save(&settings::Settings {
                phone_model_id: s.phone_model_id.clone(),
                auto_create: s.auto_create,
                only_virtual: s.only_virtual,
            });
        }
    });
}

/// 左上角图标菜单（系统菜单）加「高级模式」子菜单：两个测试入口 +「仅输出虚拟屏」勾选项。
/// 系统菜单就是点标题栏左上角图标（或右击标题栏、任务栏图标）弹出的那个菜单。
fn install_advanced_menu(hwnd: HWND) {
    unsafe {
        let Ok(sub) = CreatePopupMenu() else {
            return;
        };
        let t1 = display::wide("测试窗口");
        let t2 = display::wide("启动器");
        let t3 = display::wide("连入时仅输出虚拟屏（关闭物理屏）");
        let _ = AppendMenuW(sub, MF_STRING, IDM_ADV_TEST, PCWSTR(t1.as_ptr()));
        let _ = AppendMenuW(sub, MF_STRING, IDM_ADV_LAUNCHER, PCWSTR(t2.as_ptr()));
        let _ = AppendMenuW(sub, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(sub, MF_STRING, IDM_ADV_ONLY_VIRTUAL, PCWSTR(t3.as_ptr()));
        let sys = GetSystemMenu(hwnd, false);
        if !sys.is_invalid() {
            let label = display::wide("高级模式");
            let _ = AppendMenuW(sys, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(sys, MF_POPUP, sub.0 as usize, PCWSTR(label.as_ptr()));
        }
        MAIN.with(|c| {
            if let Some(s) = c.borrow_mut().as_mut() {
                s.adv_menu = sub;
            }
        });
    }
    sync_adv_menu_check();
}

/// 把「仅输出虚拟屏」的勾选状态同步到菜单（安装菜单/设置变更后调用）。
fn sync_adv_menu_check() {
    let (menu, on) = MAIN.with(|c| {
        c.borrow()
            .as_ref()
            .map(|s| (s.adv_menu, s.only_virtual))
            .unwrap_or_default()
    });
    if menu.is_invalid() {
        return;
    }
    unsafe {
        let _ = CheckMenuItem(
            menu,
            IDM_ADV_ONLY_VIRTUAL as u32,
            (MF_BYCOMMAND | if on { MF_CHECKED } else { MF_UNCHECKED }).0,
        );
    }
}

/// 菜单里勾/取消「连入时仅输出虚拟屏」：存设置，并当场生效——
/// 正连着且虚拟屏在就立刻关物理屏；取消时物理屏正关着就立刻恢复。
fn toggle_only_virtual() {
    let on = MAIN.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.only_virtual = !s.only_virtual;
            s.only_virtual
        } else {
            false
        }
    });
    persist_settings();
    sync_adv_menu_check();
    if on {
        ui_log("高级模式：连入时仅输出虚拟屏 = 开（主控连入后关掉物理屏，断开自动恢复）");
        if session::AGENT_PRESENT.load(Ordering::Relaxed)
            && display::list().iter().any(|d| d.is_virtual())
        {
            turn_off_physicals("菜单开启且主控正连着");
        }
    } else {
        ui_log("高级模式：连入时仅输出虚拟屏 = 关");
        restore_physicals_if_off("菜单关闭");
    }
}

/// 关掉物理屏只留虚拟屏（ccd.rs 动手，这里管标记与日志）。已关着不重复动。
fn turn_off_physicals(reason: &str) {
    let already = MAIN.with(|c| {
        c.borrow().as_ref().map(|s| s.physical_off).unwrap_or(false)
    }) || vdd::load_state().physical_off;
    if already {
        return;
    }
    ui_log(&format!("仅输出虚拟屏（{reason}）：正在关闭物理屏…"));
    match ccd::disable_physicals() {
        Ok(out) => {
            for n in &out.notes {
                ui_log(&format!("拓扑：{n}"));
            }
            if out.applied {
                let mut st = vdd::load_state();
                st.physical_off = true;
                vdd::save_state(&st);
                MAIN.with(|c| {
                    if let Some(s) = c.borrow_mut().as_mut() {
                        s.physical_off = true;
                    }
                });
                ui_log("物理屏已关闭，现在只输出虚拟屏（断开后自动恢复）");
                // 关屏把虚拟屏源位置移到了 (0,0)，资源库窗口不一定跟着走
                // （机主 2026-11-22 实测：连入后资源库没出现在虚拟屏上）：
                // 按当前枚举到的虚拟屏矩形把它钉回去，没开列表时只移悬浮按钮
                if let Some(d) = display::list().iter().find(|d| d.is_virtual()) {
                    let rect = windows::Win32::Foundation::RECT {
                        left: d.x,
                        top: d.y,
                        right: d.x + d.width as i32,
                        bottom: d.y + d.height as i32,
                    };
                    if launcher::move_to(rect) {
                        ui_log("资源库已按虚拟屏新位置重新对齐");
                    }
                }
            } else if out.changed == 0 {
                ui_log("没有可关的物理屏（本来就只剩虚拟屏？），保持现状");
            } else {
                ui_log("⚠ 关物理屏没成功，物理屏保持原样（详见上面拓扑日志）");
            }
        }
        Err(e) => ui_log(&format!("⚠ 关物理屏失败：{e}（物理屏保持原样）")),
    }
}

/// 物理屏若是被本应用关掉的就恢复它（断开/退出/启动自检/关开关时调用）。
/// 恢复没验成就不清标记：宁可下次启动再试，也不能把「还关着」这件事忘了；
/// 而且关屏时没存盘，重启机器物理屏必回来，这是最后一道保险。
fn restore_physicals_if_off(reason: &str) {
    let off = MAIN.with(|c| {
        c.borrow().as_ref().map(|s| s.physical_off).unwrap_or(false)
    }) || vdd::load_state().physical_off;
    if !off {
        return;
    }
    ui_log(&format!("恢复物理屏（{reason}）…"));
    match ccd::restore_physicals() {
        Ok(out) => {
            for n in &out.notes {
                ui_log(&format!("拓扑：{n}"));
            }
            if out.applied || out.changed == 0 {
                let mut st = vdd::load_state();
                st.physical_off = false;
                vdd::save_state(&st);
                MAIN.with(|c| {
                    if let Some(s) = c.borrow_mut().as_mut() {
                        s.physical_off = false;
                    }
                });
                ui_log("物理屏已恢复");
            } else {
                ui_log("⚠ 物理屏恢复未通过复查，标记保留（下次启动会再试；也可重启机器，关屏时未存盘、重启必恢复）");
            }
        }
        Err(e) => ui_log(&format!(
            "⚠ 物理屏恢复失败：{e}（标记保留，下次启动再试；重启机器也可恢复）"
        )),
    }
}

/// 换型号：屏在用时先按旧规格拆干净（配置手术认的是当前规格），再换规格重建。
fn on_model_changed() {
    let combo = MAIN
        .with(|c| c.borrow().as_ref().map(|s| s.combo_model))
        .unwrap_or_default();
    if combo.is_invalid() {
        return;
    }
    let idx = unsafe { SendMessageW(combo, CB_GETCURSEL, None, None) }.0 as i32;
    if idx < 0 {
        return;
    }
    let Some(model) = phones::catalog().get(idx as usize).cloned() else {
        return;
    };
    let old_id = MAIN
        .with(|c| c.borrow().as_ref().map(|s| s.phone_model_id.clone()))
        .unwrap_or_default();
    if model.id == old_id {
        return;
    }
    let has_virtual = MAIN.with(|c| {
        c.borrow()
            .as_ref()
            .map(|s| s.displays.iter().any(|d| d.is_virtual()))
            .unwrap_or(false)
    });
    if has_virtual {
        ui_log("换型号：先移除现有虚拟屏…");
        remove_phone_screen();
    }
    phones::set_current_model(&model);
    MAIN.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.phone_model_id = model.id.clone();
        }
    });
    persist_settings();
    let (w, h, hz) = phones::current_spec();
    ui_log(&format!("手机型号 → {}（虚拟屏 {}×{}@{}）", model.id, w, h, hz));
    if has_virtual {
        create_phone_screen();
    }
    refresh_displays();
}

/// 自动/手动模式切换：自动 = 主控连入建屏 + 开资源库、断开删屏；手动 = 只认按钮。
/// 切到自动时若主控已经连着，立刻补上建屏与资源库。
fn apply_auto_setting(auto: bool) {
    let changed = MAIN.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            let old = s.auto_create;
            s.auto_create = auto;
            old != auto
        } else {
            false
        }
    });
    if !changed {
        return;
    }
    persist_settings();
    if auto {
        ui_log("模式：自动（主控连入建屏并打开资源库，断开删屏）");
        let has_virtual = MAIN.with(|c| {
            c.borrow()
                .as_ref()
                .map(|s| s.displays.iter().any(|d| d.is_virtual()))
                .unwrap_or(false)
        });
        if session::AGENT_PRESENT.load(Ordering::Relaxed) && !has_virtual {
            ui_log("主控已经连着，立即创建手机屏…");
            set_screen_owner(true);
            create_phone_screen();
            open_launcher_on_virtual();
        }
    } else {
        ui_log("模式：手动（创建/移除用「创建手机屏」「移除手机屏」按钮）");
    }
    refresh_displays();
}

/// 挑目标屏：手机屏模式的虚拟屏 > 虚拟屏 > 主屏 > 第一块。
fn pick_target_display() -> Option<DisplayInfo> {
    MAIN.with(|c| {
        let cell = c.borrow();
        let s = cell.as_ref()?;
        let pick = |pred: &dyn Fn(&DisplayInfo) -> bool| s.displays.iter().find(|d| pred(d));
        pick(&|d: &DisplayInfo| d.is_virtual() && d.is_phone_mode())
            .or_else(|| pick(&|d: &DisplayInfo| d.is_virtual()))
            .or_else(|| pick(&|d: &DisplayInfo| d.primary))
            .or_else(|| s.displays.first())
            .cloned()
    })
}

fn toggle_test_window() {
    if testwin::is_open() {
        testwin::close();
        ui_log("测试窗口已关闭");
        return;
    }
    let target = pick_target_display();
    match target {
        Some(d) => {
            ui_log(&format!(
                "在 {} 上打开测试窗口（{}×{} @ {},{}）",
                d.device_name, d.width, d.height, d.x, d.y
            ));
            match testwin::open(d.x, d.y, d.width as i32, d.height as i32) {
                Ok(()) => ui_log("测试窗口已打开：默认动态模式，左上角按钮可切到静态对照"),
                Err(e) => ui_log(&format!("打开测试窗口失败：{e}")),
            }
        }
        None => ui_log("没有可用的显示器，先点「刷新显示器」"),
    }
}

/// 主控连入后把资源库打开到虚拟屏上，手机一进来就是 App 首页。
/// 已开着先关再开（可能开在别的屏上），确保落在虚拟屏；没虚拟屏就不开。
fn open_launcher_on_virtual() {
    let target = MAIN.with(|c| {
        c.borrow()
            .as_ref()
            .and_then(|s| s.displays.iter().find(|d| d.is_virtual()).cloned())
    });
    match target {
        Some(d) => {
            let rect = windows::Win32::Foundation::RECT {
                left: d.x,
                top: d.y,
                right: d.x + d.width as i32,
                bottom: d.y + d.height as i32,
            };
            ui_log(&format!(
                "自动打开资源库到虚拟屏 {}（{}×{}）",
                d.device_name, d.width, d.height
            ));
            if launcher::is_open() {
                launcher::close();
            }
            if let Err(e) = launcher::open(rect) {
                ui_log(&format!("打开资源库失败：{e}"));
            }
        }
        None => ui_log("虚拟屏没就绪，资源库暂不打开"),
    }
}

fn toggle_launcher() {
    match pick_target_display() {
        Some(d) => {
            let rect = windows::Win32::Foundation::RECT {
                left: d.x,
                top: d.y,
                right: d.x + d.width as i32,
                bottom: d.y + d.height as i32,
            };
            if !launcher::is_open() {
                ui_log(&format!(
                    "在 {} 上打开 App 资源库（{}×{}）",
                    d.device_name, d.width, d.height
                ));
            }
            launcher::toggle(rect);
        }
        None => ui_log("没有可用的显示器，先点「刷新显示器」"),
    }
}

/// 每次启动都查一遍驱动在不在，结果回主窗口：不在就直接开配置向导，不弹窗问。
fn spawn_driver_check(hwnd_raw: usize) {
    std::thread::spawn(move || {
        let found = devices::display_adapters()
            .map(|list| devices::find_vdd(&list).is_some())
            .unwrap_or(false);
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_raw as *mut _)),
                WM_DRIVER_CHECKED,
                WPARAM(if found { 1 } else { 0 }),
                LPARAM(0),
            );
        }
    });
}

/// 拆屏前把远程期间被改过尺寸/位置的窗口恢复原样（登记在 apps 里：
/// 连入时普查快照 + 全屏窗口时逐个登记，还原规则见 apps::restore_saved_places）。
fn restore_session_windows() {
    let vrect = display::list()
        .iter()
        .find(|d| d.is_virtual())
        .map(|d| windows::Win32::Foundation::RECT {
            left: d.x,
            top: d.y,
            right: d.x + d.width as i32,
            bottom: d.y + d.height as i32,
        });
    let n = apps::restore_saved_places(vrect);
    if n > 0 {
        ui_log(&format!("已把 {n} 个窗口恢复到远程开始前的尺寸与位置"));
    }
}

/// 按当前客户区重排一遍控件（模式切换后按钮行要出现/消失，跟一次布局）。
fn relayout(hwnd: HWND) {
    let mut rc = windows::Win32::Foundation::RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
    }
    layout_controls(rc.right - rc.left, rc.bottom - rc.top);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

// MARK: 窗口过程

/// 主窗口背景构图（整幅页底 + 手机屏卡片白），WM_ERASEBKGND 与 WM_PRINTCLIENT 共用：
/// 主题子控件向父窗口要背景时，给定的 DC 已按控件位置裁好，照整幅构图填即可。
fn paint_main_background(hwnd: HWND, hdc: HDC) {
    let cp = MAIN.with(|c| {
        c.borrow()
            .as_ref()
            .map(|s| s.card_phone)
            .unwrap_or((0, 0, 0, 0))
    });
    unsafe {
        let mut rc = windows::Win32::Foundation::RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        FillRect(hdc, &rc, bg_brush());
        let (x, y, w, h) = cp;
        if w > 0 && h > 0 {
            let card = windows::Win32::Foundation::RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            };
            FillRect(hdc, &card, card_brush());
        }
    }
}

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let base = WS_CHILD | WS_VISIBLE;
            let hdc = unsafe { GetDC(Some(hwnd)) };
            let dpi = unsafe { GetDeviceCaps(Some(hdc), LOGPIXELSX) };
            unsafe {
                ReleaseDC(Some(hwnd), hdc);
            }
            let ui_font = make_font("Microsoft YaHei UI", 10, 400, dpi);
            let status_font = make_font("Microsoft YaHei UI", 12, 600, dpi);
            let grp_font = make_font("Microsoft YaHei UI", 11, 600, dpi);
            let log_font = make_font("Consolas", 10, 400, dpi);
            // 自绘按钮样式（BS_OWNERDRAW）：按钮长相由 draw_button 统一画，去掉系统灰
            let btn_style = base | WINDOW_STYLE(0x0000_000B);
            // 控件先随手建在占位尺寸上，建完统一走 layout_controls 摆位。
            // 分组不再用原生分组框：父窗口在 WM_PAINT 里画白色卡片，组名只是一个粗体标签。
            let icon_box = mk_control(
                hwnd,
                "Static",
                "",
                base | WINDOW_STYLE(0x0000_0003), // SS_ICON
                0,
                0,
                10,
                10,
                9005,
                ui_font,
            );
            // 状态区左侧放应用图标（Mac 设置页同款构图）：资源 ID 1 按当前 DPI 取 40 点大小
            let hinst = HINSTANCE(
                unsafe { GetModuleHandleW(None) }
                    .map(|m| m.0)
                    .unwrap_or_default(),
            );
            if let Ok(icon) = unsafe {
                LoadImageW(
                    Some(hinst),
                    PCWSTR(1 as *mut u16),
                    IMAGE_ICON,
                    40 * dpi / 96,
                    40 * dpi / 96,
                    LR_DEFAULTCOLOR,
                )
            } {
                unsafe {
                    SendMessageW(
                        icon_box,
                        STM_SETICON,
                        Some(WPARAM(icon.0 as usize)),
                        Some(LPARAM(0)),
                    );
                }
            }
            let status = mk_control(hwnd, "Static", "", base, 0, 0, 10, 10, 9001, status_font);
            let detail = mk_control(hwnd, "Static", "", base, 0, 0, 10, 10, 9006, ui_font);
            let list = mk_control(
                hwnd,
                "SysListView32",
                "",
                base | WS_BORDER | WINDOW_STYLE(LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS),
                0,
                0,
                10,
                10,
                9002,
                ui_font,
            );
            init_list_columns(list, dpi);
            // 本应用设置：选的型号决定当前规格（配置手术、模式判定都读 phones 的当前规格）
            let saved = settings::load();
            let model =
                phones::model_by_id(&saved.phone_model_id).unwrap_or_else(phones::standard_model);
            phones::set_current_model(&model);
            let lbl_phone = mk_control(
                hwnd,
                "Static",
                "手机屏",
                base,
                0,
                0,
                10,
                10,
                9010,
                grp_font,
            );
            let btn_create = mk_control(
                hwnd,
                "Button",
                "创建手机屏",
                btn_style,
                0,
                0,
                10,
                10,
                ID_CREATE,
                ui_font,
            );
            let btn_remove = mk_control(
                hwnd,
                "Button",
                "移除手机屏",
                btn_style,
                0,
                0,
                10,
                10,
                ID_REMOVE,
                ui_font,
            );
            let phone_hint = mk_control(
                hwnd,
                "Static",
                "向日葵中增加一块竖屏；移除会禁用驱动，创建时重新启用",
                base,
                0,
                0,
                10,
                10,
                9011,
                ui_font,
            );
            let model_label = mk_control(
                hwnd,
                "Static",
                "手机型号",
                base,
                0,
                0,
                10,
                10,
                9016,
                ui_font,
            );
            let combo_model = mk_control(
                hwnd,
                "ComboBox",
                "",
                base | WS_VSCROLL | WINDOW_STYLE(0x0003), // CBS_DROPDOWNLIST
                0,
                0,
                10,
                10,
                ID_MODEL,
                ui_font,
            );
            for m in phones::catalog() {
                let name_w = display::wide(&m.id);
                unsafe {
                    SendMessageW(
                        combo_model,
                        CB_ADDSTRING,
                        None,
                        Some(LPARAM(name_w.as_ptr() as isize)),
                    );
                }
            }
            let sel = phones::catalog()
                .iter()
                .position(|m| m.id == model.id)
                .unwrap_or(0);
            unsafe {
                SendMessageW(combo_model, CB_SETCURSEL, Some(WPARAM(sel)), None);
            }
            let radio_auto = mk_control(
                hwnd,
                "Button",
                "自动：主控连入建屏并打开资源库，断开删屏",
                base | WS_GROUP | WINDOW_STYLE(0x0009), // BS_AUTORADIOBUTTON
                0,
                0,
                10,
                10,
                ID_MODE_AUTO,
                ui_font,
            );
            let radio_manual = mk_control(
                hwnd,
                "Button",
                "手动：用上面的按钮建屏、删屏",
                base | WINDOW_STYLE(0x0009), // BS_AUTORADIOBUTTON
                0,
                0,
                10,
                10,
                ID_MODE_MANUAL,
                ui_font,
            );
            unsafe {
                SendMessageW(
                    if saved.auto_create {
                        radio_auto
                    } else {
                        radio_manual
                    },
                    BM_SETCHECK,
                    Some(WPARAM(1)),
                    None,
                );
            }
            let btn_refresh = mk_control(
                hwnd,
                "Button",
                "刷新显示器",
                btn_style,
                0,
                0,
                10,
                10,
                ID_REFRESH,
                ui_font,
            );
            // 列表配色跟着主题走：白底深字（原生默认也是白底，这里把文字色钉死）
            unsafe {
                SendMessageW(
                    list,
                    LVM_SETBKCOLOR,
                    Some(WPARAM(0)),
                    Some(LPARAM(theme_card().0 as isize)),
                );
                SendMessageW(
                    list,
                    LVM_SETTEXTBKCOLOR,
                    Some(WPARAM(0)),
                    Some(LPARAM(theme_card().0 as isize)),
                );
                SendMessageW(
                    list,
                    LVM_SETTEXTCOLOR,
                    Some(WPARAM(0)),
                    Some(LPARAM(rgb(29, 29, 31).0 as isize)),
                );
            }
            let log_edit = mk_control(
                hwnd,
                "Edit",
                "",
                base | WS_BORDER | WS_VSCROLL
                    | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32),
                0,
                0,
                10,
                10,
                9003,
                log_font,
            );
            MAIN.with(|c| {
                *c.borrow_mut() = Some(MainState {
                    status,
                    list,
                    btn_refresh,
                    btn_create,
                    btn_remove,
                    icon_box,
                    detail,
                    lbl_phone,
                    card_phone: (0, 0, 0, 0),
                    combo_model,
                    radio_auto,
                    radio_manual,
                    model_label,
                    phone_model_id: model.id.clone(),
                    auto_create: saved.auto_create,
                    only_virtual: saved.only_virtual,
                    physical_off: vdd::load_state().physical_off,
                    adv_menu: HMENU::default(),
                    screen_auto_owned: false,
                    phone_hint,
                    log_edit,
                    log_text: String::new(),
                    displays: Vec::new(),
                    dpi,
                    status_level: StatusLevel::Neutral,
                });
            });
            let mut rc = windows::Win32::Foundation::RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut rc);
            }
            layout_controls(rc.right - rc.left, rc.bottom - rc.top);
            ui_log(&format!(
                "Snug for Sunlogin — Windows 版 v{}（Rust）",
                env!("SNUG_VERSION")
            ));
            ui_log(&format!("日志文件：{}", log::dir().join("snug-windows.log").display()));
            ui_log("范围：只做虚拟屏与串流测试，不做手势翻译");
            refresh_displays();
            let (sw, sh, shz) = phones::current_spec();
            ui_log(&format!(
                "手机型号：{}（虚拟屏 {}×{}@{}）",
                model.id, sw, sh, shz
            ));
            ui_log(if saved.auto_create {
                "自动建屏：开（盯 desktopagent 进程，主控连入自动创建、断开自动移除）"
            } else {
                "自动建屏：关"
            });
            install_advanced_menu(hwnd);
            // 单选钮在创建时就擦过一次背景，那时卡片位置还没定，主题控件代画父背景
            // 擦到的是页底灰（灰带）；布局落定后强制全体子控件重擦重画一次才消失
            let _ = RedrawWindow(
                Some(hwnd),
                None,
                None,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
            );
            ui_log(if saved.only_virtual {
                "高级模式：连入时仅输出虚拟屏（在左上角图标菜单的「高级模式」里可关）"
            } else {
                "高级模式：关（左上角图标菜单的「高级模式」里可开「连入时仅输出虚拟屏」）"
            });
            // 异常退出兜底：上次把物理屏关了没恢复（标记还在），先恢复再干别的
            if vdd::load_state().physical_off {
                ui_log("检测到上次物理屏未恢复（程序可能异常退出），立即恢复…");
                restore_physicals_if_off("启动自检");
            }
            session::spawn(hwnd.0 as usize);
            // 启动后静默查一次新版：没有就不打扰，有才弹窗问
            update::spawn_check(hwnd.0 as usize, false);
            // 配置向导：带 --guide 启动直接打开；否则每次启动都检测一遍驱动，
            // 状态区先挂「驱动检测中」，不在就直接开向导（不弹窗问）
            if GUIDE_REQUESTED.load(Ordering::Relaxed) {
                guide::open(hwnd);
            } else {
                let (status_h, detail_h) = MAIN.with(|c| {
                    c.borrow()
                        .as_ref()
                        .map(|s| (s.status, s.detail))
                        .unwrap_or_default()
                });
                set_text(status_h, "驱动检测中…");
                set_text(detail_h, "正在确认虚拟显示驱动（MttVDD）是否就绪");
                spawn_driver_check(hwnd.0 as usize);
            }
            LRESULT(0)
        }
        WM_SIZE => {
            let w = (lparam.0 & 0xffff) as i32;
            let h = ((lparam.0 >> 16) & 0xffff) as i32;
            if w > 0 && h > 0 {
                layout_controls(w, h);
            }
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let dpi = MAIN.with(|c| c.borrow().as_ref().map(|s| s.dpi).unwrap_or(96));
            let mmi = lparam.0 as *mut MINMAXINFO;
            unsafe {
                (*mmi).ptMinTrackSize.x = 600 * dpi / 96;
                (*mmi).ptMinTrackSize.y = 560 * dpi / 96;
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            paint_main_background(hwnd, HDC(wparam.0 as *mut core::ffi::c_void));
            LRESULT(1)
        }
        WM_PRINTCLIENT => {
            // 主题子控件（单选钮）向父窗口要身下的背景走的是 WM_PRINTCLIENT
            // （DrawThemeParentBackground），不是 WM_ERASEBKGND：父窗口不处理时，
            // 控件拿到的是系统按钮面灰 #F0F0F0——这才是单选钮灰带的真身
            // （2026-11-22 实测：CTLCOLORBTN 改白、ERASEBKGND 按卡片构图擦、启动后
            // RedrawWindow 全体重擦，三招都不动它，最后定位到这里）。
            paint_main_background(hwnd, HDC(wparam.0 as *mut core::ffi::c_void));
            LRESULT(0)
        }
        WM_PAINT => {
            // 现代主题的底：整幅刷浅灰，再把内容卡片画成白底圆角描边
            let cp = MAIN.with(|c| {
                c.borrow()
                    .as_ref()
                    .map(|s| s.card_phone)
                    .unwrap_or((0, 0, 0, 0))
            });
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut rc = windows::Win32::Foundation::RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                FillRect(hdc, &rc, bg_brush());
                for (x, y, w, h) in [cp] {
                    if w > 0 && h > 0 {
                        let brush = CreateSolidBrush(theme_card());
                        let pen = CreatePen(PS_SOLID, 1, rgb(226, 226, 231));
                        let ob = SelectObject(hdc, HGDIOBJ(brush.0));
                        let op = SelectObject(hdc, HGDIOBJ(pen.0));
                        let _ = RoundRect(hdc, x, y, x + w, y + h, 14, 14);
                        SelectObject(hdc, ob);
                        SelectObject(hdc, op);
                        let _ = DeleteObject(HGDIOBJ(brush.0));
                        let _ = DeleteObject(HGDIOBJ(pen.0));
                    }
                }
                let _ = EndPaint(hwnd, &mut ps);
            }
            LRESULT(0)
        }
        WM_DRAWITEM => {
            let dis = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
            draw_button(dis);
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC => {
            // 静态文字配色：状态标题按级别（就绪绿、待处理橙），说明/提示次灰，
            // 组名与标签深色；底刷按所在区域给（顶区用底色刷、卡片内用白刷），不再发灰。
            let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
            let ctrl = HWND(lparam.0 as *mut core::ffi::c_void);
            // 只读 Edit（日志框）的配色也走这条消息而不是 WM_CTLCOLOREDIT
            // （Win32 惯例），先单独处理：白底深字、不透明底色把字底刷实
            let is_log = MAIN.with(|c| {
                c.borrow().as_ref().map(|s| s.log_edit == ctrl).unwrap_or(false)
            });
            if is_log {
                unsafe {
                    SetTextColor(hdc, rgb(29, 29, 31));
                    SetBkColor(hdc, theme_card());
                }
                return LRESULT(card_brush().0 as isize);
            }
            // kind：0 无关、1 状态标题、2 次灰、3 深色
            let (kind, level, on_card) = MAIN.with(|c| {
                let cell = c.borrow();
                match cell.as_ref() {
                    Some(s) => {
                        if s.status == ctrl {
                            (1, s.status_level, false)
                        } else if s.detail == ctrl {
                            (2, s.status_level, false)
                        } else if s.phone_hint == ctrl {
                            (2, s.status_level, true)
                        } else if s.lbl_phone == ctrl || s.model_label == ctrl {
                            (3, s.status_level, true)
                        } else {
                            (0, s.status_level, false)
                        }
                    }
                    None => (0, StatusLevel::Neutral, false),
                }
            });
            if kind == 0 {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            let color = match (kind, level) {
                (1, StatusLevel::Ready) => rgb(31, 122, 61),
                (1, StatusLevel::Warn) => rgb(156, 106, 0),
                (2, _) => rgb(110, 110, 115),
                _ => rgb(29, 29, 31),
            };
            unsafe {
                SetTextColor(hdc, color);
                SetBkMode(hdc, TRANSPARENT);
            }
            LRESULT(if on_card {
                card_brush().0 as isize
            } else {
                bg_brush().0 as isize
            })
        }
        WM_CTLCOLORBTN => {
            // 单选钮（自绘按钮不走这里）：深色字、白底，与卡片融为一体。
            // 不能用 TRANSPARENT：字底会透出父窗口的页底灰（#F5F5F7），
            // 看起来就是「单选钮底色发灰」（2026-11-21 机主实报）；改不透明，
            // 字底用卡片白刷实，才真正贴在白卡片上。
            let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
            unsafe {
                SetTextColor(hdc, rgb(29, 29, 31));
                SetBkColor(hdc, theme_card());
            }
            LRESULT(card_brush().0 as isize)
        }
        WM_CTLCOLOREDIT => {
            // 日志框：白底深字
            let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
            unsafe {
                SetTextColor(hdc, rgb(29, 29, 31));
                SetBkColor(hdc, theme_card());
            }
            LRESULT(card_brush().0 as isize)
        }
        WM_DPICHANGED => {
            // 拖到不同 DPI 的屏上：更新缩放系数并按系统建议的新尺寸摆好
            // （字体仍是创建时磅数，不跟着重建——单屏笔记本上碰不到，够用）
            let new_dpi = (wparam.0 & 0xffff) as i32;
            MAIN.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.dpi = new_dpi;
                }
            });
            let rect = lparam.0 as *const windows::Win32::Foundation::RECT;
            unsafe {
                let r = *rect;
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER,
                );
            }
            LRESULT(0)
        }
        session::WM_SESSION_START => {
            ui_log("检测到向日葵主控连入（desktopagent 进程已启动）");
            // 先给所有窗口拍一张「远程前」的快照：期间被改过的尺寸，断开时照此还原
            apps::snapshot_all_windows();
            let (auto, has_virtual) = MAIN.with(|c| {
                c.borrow()
                    .as_ref()
                    .map(|s| (s.auto_create, s.displays.iter().any(|d| d.is_virtual())))
                    .unwrap_or((false, false))
            });
            if auto {
                if !has_virtual {
                    ui_log("自动创建手机屏…");
                    set_screen_owner(true);
                    create_phone_screen();
                }
                // 屏就绪后资源库也跟上：手机一连进来就是 App 首页，不用再点悬浮键
                open_launcher_on_virtual();
            } else {
                refresh_displays();
            }
            // 高级模式：虚拟屏在场就把物理屏关掉，只输出虚拟屏（手动模式下
            // 会话开始不建屏，但屏若已手动建好同样生效）；断开时在 END 里恢复
            let only_virtual = MAIN.with(|c| {
                c.borrow().as_ref().map(|s| s.only_virtual).unwrap_or(false)
            });
            if only_virtual {
                if display::list().iter().any(|d| d.is_virtual()) {
                    turn_off_physicals("主控连入");
                } else {
                    ui_log("仅输出虚拟屏：虚拟屏未就绪，暂不关物理屏");
                }
            }
            LRESULT(0)
        }
        session::WM_SESSION_END => {
            ui_log("向日葵主控已断开（desktopagent 进程已退出）");
            // 物理屏恢复放在最前面且不设条件：只要是本应用关的就先恢复，
            // 后面的拆屏/还原流程再出什么问题也不能把用户留在无物理屏状态
            restore_physicals_if_off("主控断开");
            // 虚拟屏在不在用新鲜枚举判：状态里的那份是上次刷新留下的缓存，
            // 拿它决定拆不拆，缓存一旧就会拆漏
            let has_virtual = display::list().iter().any(|d| d.is_virtual());
            let (auto, owned) = MAIN.with(|c| {
                c.borrow()
                    .as_ref()
                    .map(|s| (s.auto_create, s.screen_auto_owned))
                    .unwrap_or((false, false))
            });
            // 只拆自动建的：手动建的屏留给用户自己决定
            if auto && owned {
                ui_log("自动关闭资源库并还原窗口…");
                launcher::close_all();
                // 先还原窗口再拆屏：屏一拆，留在上面的窗口会被系统胡乱塞位置。
                // 还原不以「虚拟屏还在」为门：向日葵会在会话结束时先把虚拟屏摘出
                // 拓扑（2026-11-21 实证），等我们处理时屏常常已经不在了——被全屏过
                // 的窗口（touched）照样能按登记还原，普查快照的没了参照系才跳过。
                restore_session_windows();
                if has_virtual {
                    remove_phone_screen();
                } else {
                    ui_log("虚拟屏已被向日葵摘除，跳过拆屏");
                }
                set_screen_owner(false);
            } else {
                // 不拆也要把三个条件的值打出来，下次一看日志就知道卡在哪个条件
                let yn = |b: bool| if b { "是" } else { "否" };
                ui_log(&format!(
                    "主控已断开，但不自动拆屏（自动模式={}，本屏为自动建={}，虚拟屏在={}）",
                    yn(auto),
                    yn(owned),
                    yn(has_virtual)
                ));
                refresh_displays();
            }
            LRESULT(0)
        }
        WM_DRIVER_CHECKED => {
            refresh_displays();
            if wparam.0 == 1 {
                ui_log("驱动检测：MttVDD 已就绪");
            } else {
                ui_log("驱动检测：没找到 MttVDD 设备，自动打开配置向导");
                guide::open(hwnd);
            }
            LRESULT(0)
        }
        update::WM_UPDATE_AVAILABLE => {
            if let Some(latest) = update::take_pending() {
                let text = format!(
                    "发现新版本 v{}（当前 v{}）。\n\n现在下载并更新吗？下载完程序会重启一次完成替换。",
                    latest.version,
                    update::current_version()
                );
                let text_w = display::wide(&text);
                let cap_w = display::wide("SnugWindows 更新");
                let yes = unsafe {
                    MessageBoxW(
                        Some(hwnd),
                        PCWSTR(text_w.as_ptr()),
                        PCWSTR(cap_w.as_ptr()),
                        MB_YESNO | MB_ICONQUESTION,
                    )
                };
                if yes == IDYES {
                    ui_log(&format!("开始下载更新 v{}…", latest.version));
                    update::spawn_prepare(hwnd.0 as usize, latest);
                } else {
                    ui_log("已取消更新（下次启动或点「检查更新」还会再问）");
                }
            }
            LRESULT(0)
        }
        update::WM_UPDATE_NONE => {
            ui_log(&format!("已是最新版（v{}）", update::current_version()));
            LRESULT(0)
        }
        update::WM_UPDATE_FAILED => {
            if let Some(e) = update::take_failure() {
                ui_log(&format!("更新没成：{e}"));
            }
            LRESULT(0)
        }
        update::WM_UPDATE_READY => {
            ui_log("新版已下载并验好，正在重启完成更新…");
            match update::launch_swap() {
                Ok(()) => unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                },
                Err(e) => ui_log(&format!("更新没成：{e}")),
            }
            LRESULT(0)
        }
        WM_SYSCOMMAND => {
            // 左上角图标菜单里「高级模式」的自定义项在这里处理，其余（关闭/移动等）走默认
            match wparam.0 & 0xFFF0 {
                IDM_ADV_TEST => {
                    toggle_test_window();
                    LRESULT(0)
                }
                IDM_ADV_LAUNCHER => {
                    toggle_launcher();
                    LRESULT(0)
                }
                IDM_ADV_ONLY_VIRTUAL => {
                    toggle_only_virtual();
                    LRESULT(0)
                }
                _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
            }
        }
        WM_COMMAND => {
            let id = (wparam.0 as u32 & 0xffff) as i32;
            let notify = (wparam.0 as u32 >> 16) as u32;
            match id {
                ID_REFRESH => refresh_displays(),
                ID_CREATE => {
                    set_screen_owner(false);
                    create_phone_screen();
                }
                ID_REMOVE => {
                    set_screen_owner(false);
                    restore_session_windows();
                    remove_phone_screen();
                }
                ID_MODEL if notify == 1 => on_model_changed(), // 1 = CBN_SELCHANGE
                ID_MODE_AUTO => {
                    apply_auto_setting(true);
                    relayout(hwnd);
                }
                ID_MODE_MANUAL => {
                    apply_auto_setting(false);
                    relayout(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // 退出前同样兜底：物理屏关着就先恢复（正常断开时 END 已恢复，这里是保险）
            restore_physicals_if_off("程序退出");
            testwin::close();
            launcher::close_all();
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
