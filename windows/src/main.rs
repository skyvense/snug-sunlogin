//! Snug for Sunlogin — Windows 版 spike（Rust）
//!
//! 只验证一件事：向日葵手机端能不能用一块 Windows 虚拟屏（VDD），串流是否流畅。
//! 本工具做四件小事：枚举显示器并把 VDD 虚拟屏设成手机屏模式（1080×2346@60）；
//! 按需改 VDD 配置并重启驱动设备（可原样恢复）；在虚拟屏上开动/静对照的测试窗口；
//! 在虚拟屏上开 App 资源库（启动器）：运行中/全部两页 + 搜索，选中 App 自动全屏到虚拟屏。
//! 不做手势翻译（机主 2026-11-19 定）。

#![windows_subsystem = "windows"]

mod apps;
mod devices;
mod display;
mod launcher;
mod log;
mod testwin;
mod vdd;

use display::{DisplayInfo, PHONE_H, PHONE_HZ, PHONE_W};
use std::cell::RefCell;
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const ID_REFRESH: i32 = 1001;
const ID_CREATE: i32 = 1002;
const ID_REMOVE: i32 = 1003;
const ID_TEST: i32 = 1004;
const ID_INSTALL: i32 = 1005;
const ID_LAUNCHER: i32 = 1006;

struct MainState {
    status: HWND,
    list: HWND,
    log_edit: HWND,
    log_text: String,
    displays: Vec<DisplayInfo>,
}

thread_local! {
    static MAIN: RefCell<Option<MainState>> = const { RefCell::new(None) };
}

fn main() {
    log::init();
    // GUI 子系统下 panic 默认无处可见，统一落到日志文件，方便事后排障
    std::panic::set_hook(Box::new(|info| {
        log::write(&format!("PANIC：{info}"));
    }));
    std::process::exit(run());
}

fn run() -> i32 {
    unsafe {
        let hinstance = match GetModuleHandleW(None) {
            Ok(m) => HINSTANCE(m.0),
            Err(_) => HINSTANCE::default(),
        };
        let class_w = display::wide("SnugSunloginMainWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(main_wnd_proc),
            hInstance: hinstance,
            lpszClassName: PCWSTR(class_w.as_ptr()),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(6 as *mut core::ffi::c_void), // COLOR_WINDOW + 1
            ..Default::default()
        };
        RegisterClassW(&wc);
        let title_w = display::wide("Snug for Sunlogin — Windows 测试版（Rust）");
        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            80,
            80,
            660,
            730,
            None,
            None,
            Some(hinstance),
            None,
        ) {
            Ok(h) => h,
            Err(_) => return 1,
        };
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

fn mk_control(
    parent: HWND,
    class: &str,
    text: &str,
    style: WINDOW_STYLE,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
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
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
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

fn status_text(displays: &[DisplayInfo]) -> String {
    if let Some(d) = displays.iter().find(|d| d.is_virtual() && d.is_phone_mode()) {
        return format!(
            "手机屏已就绪：{}×{}@{}，向日葵手机端应能选到这块屏",
            d.width, d.height, d.hz
        );
    }
    if let Some(d) = displays.iter().find(|d| d.is_virtual()) {
        return format!(
            "VDD 虚拟屏在（{}×{}@{}），还不是手机屏模式，点「创建手机屏」",
            d.width, d.height, d.hz
        );
    }
    if vdd::settings_exists() {
        return "VDD 配置文件在，但没枚举到虚拟屏：驱动可能没装好或被禁用".to_string();
    }
    "未检测到 VDD：先点「安装 VDD」，装完再点「创建手机屏」".to_string()
}

fn refresh_displays() {
    let displays = display::list();
    let (list, status) = MAIN.with(|c| {
        let mut cell = c.borrow_mut();
        match cell.as_mut() {
            Some(s) => {
                s.displays = displays.clone();
                (s.list, s.status)
            }
            None => (HWND::default(), HWND::default()),
        }
    });
    if !list.is_invalid() {
        unsafe {
            SendMessageW(list, LB_RESETCONTENT, Some(WPARAM(0)), Some(LPARAM(0)));
        }
        for d in &displays {
            let w = display::wide(&d.describe());
            unsafe {
                SendMessageW(
                    list,
                    LB_ADDSTRING,
                    Some(WPARAM(0)),
                    Some(LPARAM(w.as_ptr() as isize)),
                );
            }
        }
    }
    let st = status_text(&displays);
    if !status.is_invalid() {
        set_text(status, &st);
    }
    ui_log(&format!("显示器枚举：{} 块", displays.len()));
    for d in &displays {
        ui_log(&format!("  {}", d.describe()));
    }
    ui_log(&format!("状态：{st}"));
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

fn create_phone_screen() {
    ui_log("—— 创建手机屏 ——");
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
            ui_log("配置里已有 1080×2346 分辨率，不重复加");
        } else {
            text = vdd::add_phone_resolution(&text);
            state.added_resolution = true;
            ui_log("已往配置加入 1080×2346@60 分辨率");
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

    if !restart_vdd_device() {
        return;
    }
    // 重启后虚拟屏要几秒才挂上桌面（真机实测约 2 秒，慢的机器更久）：
    // 轮询等它出现，别用一次固定等待赌时间，否则会误报「没枚举到」。
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if display::list().iter().any(|d| d.is_virtual()) {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        wait_pump(500);
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
            ui_log("完成：手机屏已就绪（1080×2346@60）。用向日葵手机端连这台电脑、选这块屏测试");
        }
        Some(d) => {
            ui_log(&format!(
                "虚拟屏当前 {}×{}@{}，设置成手机屏模式…",
                d.width, d.height, d.hz
            ));
            let r = display::set_mode(&d.device_name, PHONE_W, PHONE_H, PHONE_HZ);
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
                ui_log("完成：手机屏已就绪（1080×2346@60）。用向日葵手机端连这台电脑、选这块屏测试");
            } else {
                ui_log("设模式后仍不是 1080×2346，看上面枚举结果确认虚拟屏实际模式，并把日志贴回主会话");
            }
        }
        None => {
            ui_log("重启后仍没枚举到虚拟屏：打开设备管理器看 Virtual Display Driver 的状态，把日志贴回主会话");
        }
    }
}

fn remove_phone_screen() {
    ui_log("—— 移除手机屏（恢复配置） ——");
    let state = vdd::load_state();
    if state.created_file && vdd::settings_exists() {
        // 配置文件是本应用新建的：整个删掉就是最干净的回滚
        match std::fs::remove_file(vdd::SETTINGS_PATH) {
            Ok(()) => ui_log("已删除本应用新建的配置文件"),
            Err(e) => ui_log(&format!("删配置文件失败：{e}")),
        }
    } else if vdd::settings_exists() {
        match vdd::load_settings() {
            Ok(text) => {
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
    if restart_vdd_device() {
        wait_pump(1500);
    }
    refresh_displays();
    ui_log("移除完成（原始配置的完整备份仍在 vdd_settings.xml.snug-pristine.bak）");
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

fn install_vdd() {
    let exe = std::env::current_exe().unwrap_or_default();
    let script = exe.with_file_name("setup-virtual-display.ps1");
    if script.exists() {
        match std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script.to_string_lossy(),
            ])
            .spawn()
        {
            Ok(_) => ui_log("已打开虚拟屏搞通脚本窗口：它会自己提权，把驱动装完（官方包 + nefcon 建节点）并验证虚拟屏真的出现；跑完看它的结论，再回来点「创建手机屏」"),
            Err(e) => ui_log(&format!("启动搞通脚本失败：{e}")),
        }
    } else {
        ui_log("exe 旁边没找到 setup-virtual-display.ps1：它与 exe 在同一个发布包里，确认解压完整");
    }
}

// MARK: 窗口过程

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let base = WS_CHILD | WS_VISIBLE;
            let status = mk_control(hwnd, "Static", "", base, 14, 12, 620, 22, 9001);
            let list = mk_control(
                hwnd,
                "ListBox",
                "",
                base | WS_BORDER | WS_VSCROLL | WINDOW_STYLE(LBS_NOTIFY as u32),
                14,
                40,
                620,
                128,
                9002,
            );
            mk_control(hwnd, "Button", "刷新显示器", base, 14, 178, 110, 32, ID_REFRESH);
            mk_control(hwnd, "Button", "创建手机屏", base, 132, 178, 140, 32, ID_CREATE);
            mk_control(hwnd, "Button", "移除手机屏", base, 280, 178, 140, 32, ID_REMOVE);
            mk_control(hwnd, "Button", "测试窗口", base, 428, 178, 100, 32, ID_TEST);
            mk_control(hwnd, "Button", "启动器", base, 536, 178, 98, 32, ID_LAUNCHER);
            mk_control(hwnd, "Button", "安装 VDD", base, 14, 218, 150, 32, ID_INSTALL);
            mk_control(
                hwnd,
                "Static",
                "装驱动只需一次；创建/移除手机屏会自动改配置并重启驱动设备",
                base,
                176,
                224,
                458,
                22,
                9004,
            );
            let log_edit = mk_control(
                hwnd,
                "Edit",
                "",
                base | WS_BORDER | WS_VSCROLL
                    | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32),
                14,
                258,
                620,
                414,
                9003,
            );
            MAIN.with(|c| {
                *c.borrow_mut() = Some(MainState {
                    status,
                    list,
                    log_edit,
                    log_text: String::new(),
                    displays: Vec::new(),
                });
            });
            ui_log("Snug for Sunlogin — Windows 测试版 v0.1.0（Rust）");
            ui_log(&format!("日志文件：{}", log::dir().join("snug-windows.log").display()));
            ui_log("范围：只做虚拟屏与串流测试，不做手势翻译");
            refresh_displays();
            LRESULT(0)
        }
        WM_COMMAND => {
            match (wparam.0 as u32 & 0xffff) as i32 {
                ID_REFRESH => refresh_displays(),
                ID_CREATE => create_phone_screen(),
                ID_REMOVE => remove_phone_screen(),
                ID_TEST => toggle_test_window(),
                ID_LAUNCHER => toggle_launcher(),
                ID_INSTALL => install_vdd(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
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
