//! 配置向导：驱动没装、虚拟屏用不了时，一步步带用户搞通——
//! 检查驱动 → 安装驱动（跑随包的 setup-virtual-display.ps1，输出实时进窗口）→
//! 创建手机屏并验证它真的出现。主程序带 --guide 启动时直接打开；主窗口的
//! 「配置向导」按钮、以及启动时检测到没驱动的询问框也能打开它。

use crate::devices;
use crate::display::{self, wide};
use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{EM_SCROLLCARET, EM_SETSEL};
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS_NAME: &str = "SnugSunloginGuideWindow";
const ID_ACTION: i32 = 3002;
const ID_CLOSE: i32 = 3003;

const WM_GUIDE_CHECKED: u32 = WM_APP + 31;
const WM_GUIDE_LOG: u32 = WM_APP + 32;
const WM_GUIDE_INSTALL_DONE: u32 = WM_APP + 33;
const WM_GUIDE_VERIFIED: u32 = WM_APP + 34;

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Check,
    Install,
    Verify,
    Done,
}

#[derive(Clone)]
struct CheckResult {
    /// 找到的 MttVDD 设备描述（名字 + 实例 ID）；没找到为 None
    device: Option<String>,
    virtual_now: bool,
    phone_now: bool,
}

#[derive(Default)]
struct Inbox {
    check: Option<CheckResult>,
    log: Vec<String>,
    install_exit: Option<i32>,
    verify: Option<u32>,
}

static INBOX: Mutex<Inbox> = Mutex::new(Inbox {
    check: None,
    log: Vec::new(),
    install_exit: None,
    verify: None,
});

struct GuideState {
    hwnd: HWND,
    body: HWND,
    action: HWND,
    step: Step,
    busy: bool,
    device_ok: bool,
    phone_ok: bool,
    after_install: bool,
    text: String,
}

thread_local! {
    static GUIDE: RefCell<Option<GuideState>> = const { RefCell::new(None) };
}

static REGISTERED: AtomicBool = AtomicBool::new(false);

fn make_font(height: i32, weight: i32) -> HFONT {
    let mut lf = LOGFONTW::default();
    lf.lfHeight = height;
    lf.lfWeight = weight;
    let face = wide("Microsoft YaHei UI");
    lf.lfFaceName[..face.len() - 1].copy_from_slice(&face[..face.len() - 1]);
    unsafe { CreateFontIndirectW(&lf) }
}

fn post(hwnd_raw: usize, msg: u32, wparam: usize) {
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd_raw as *mut _)),
            msg,
            WPARAM(wparam),
            LPARAM(0),
        );
    }
}

/// 打开向导（已打开就提到最前）。
pub fn open(parent: HWND) {
    let existing = GUIDE.with(|c| c.borrow().as_ref().map(|s| s.hwnd));
    if let Some(hwnd) = existing {
        unsafe {
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }
    unsafe {
        let hinstance = HINSTANCE(GetModuleHandleW(None).map(|m| m.0).unwrap_or_default());
        if !REGISTERED.swap(true, Ordering::SeqCst) {
            let class_w = wide(CLASS_NAME);
            let wc = WNDCLASSW {
                lpfnWndProc: Some(guide_wnd_proc),
                hInstance: hinstance,
                lpszClassName: PCWSTR(class_w.as_ptr()),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut _),
                ..Default::default()
            };
            RegisterClassW(&wc);
        }
        let dpi = GetDeviceCaps(Some(GetDC(None)), LOGPIXELSX);
        let sc = |v: i32| v * dpi / 96;
        let mut px = 120;
        let mut py = 120;
        let mut prect = RECT::default();
        if GetWindowRect(parent, &mut prect).is_ok() {
            px = prect.left + sc(40);
            py = prect.top + sc(40);
        }
        let class_w = wide(CLASS_NAME);
        let title_w = wide("配置向导 — 把虚拟屏一步步搞通");
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            px,
            py,
            sc(700),
            sc(600),
            Some(parent),
            None,
            Some(hinstance),
            None,
        );
    }
}

/// 启用/禁用按钮：windows 0.59 里 EnableWindow 不在常用路径下，直接改 WS_DISABLED 样式位
fn set_enabled(hwnd: HWND, on: bool) {
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let new = if on {
            style & !WS_DISABLED.0
        } else {
            style | WS_DISABLED.0
        };
        SetWindowLongW(hwnd, GWL_STYLE, new as i32);
    }
}

fn set_action(label: &str, enabled: bool) {
    let action = GUIDE.with(|c| c.borrow().as_ref().map(|s| s.action));
    if let Some(action) = action {
        let text_w = wide(label);
        unsafe {
            SetWindowTextW(action, PCWSTR(text_w.as_ptr()));
        }
        set_enabled(action, enabled);
    }
}

fn render_body() {
    let (body, text) = GUIDE.with(|c| {
        let cell = c.borrow();
        match cell.as_ref() {
            Some(s) => (Some(s.body), s.text.clone()),
            None => (None, String::new()),
        }
    });
    if let Some(body) = body {
        let text_w = wide(&text.replace('\n', "\r\n"));
        unsafe {
            SetWindowTextW(body, PCWSTR(text_w.as_ptr()));
            let len = GetWindowTextLengthW(body);
            SendMessageW(
                body,
                EM_SETSEL,
                Some(WPARAM(len as usize)),
                Some(LPARAM(len as isize)),
            );
            SendMessageW(body, EM_SCROLLCARET, Some(WPARAM(0)), Some(LPARAM(0)));
        }
    }
}

fn set_text(text: &str) {
    GUIDE.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.text = text.to_string();
        }
    });
    render_body();
}

fn append_line(line: &str) {
    GUIDE.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            if !s.text.is_empty() {
                s.text.push('\n');
            }
            s.text.push_str(line);
        }
    });
    render_body();
}

fn do_check() -> CheckResult {
    let device = match devices::display_adapters() {
        Ok(list) => devices::find_vdd(&list)
            .map(|d| format!("{}（{}）", d.friendly_name, d.instance_id)),
        Err(_) => None,
    };
    let list = display::list();
    CheckResult {
        device,
        virtual_now: list.iter().any(|d| d.is_virtual()),
        phone_now: list.iter().any(|d| d.is_virtual() && d.is_phone_mode()),
    }
}

fn start_check(hwnd_raw: usize) {
    std::thread::spawn(move || {
        let r = do_check();
        INBOX.lock().unwrap().check = Some(r);
        post(hwnd_raw, WM_GUIDE_CHECKED, 0);
    });
}

fn push_log(line: String, hwnd_raw: usize) {
    INBOX.lock().unwrap().log.push(line);
    post(hwnd_raw, WM_GUIDE_LOG, 0);
}

fn start_install(hwnd_raw: usize) {
    std::thread::spawn(move || {
        let script = std::env::current_exe()
            .unwrap_or_default()
            .with_file_name("setup-virtual-display.ps1");
        if !script.exists() {
            push_log(
                "exe 旁边没找到 setup-virtual-display.ps1：它与 exe 在同一个发布包里，确认解压完整".to_string(),
                hwnd_raw,
            );
            INBOX.lock().unwrap().install_exit = Some(-1);
            post(hwnd_raw, WM_GUIDE_INSTALL_DONE, 0);
            return;
        }
        push_log(format!("开始运行 {}", script.display()), hwnd_raw);
        let mut cmd = Command::new("powershell.exe");
        cmd.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.to_string_lossy(),
            "-NoPause",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
        match cmd.spawn() {
            Ok(mut child) => {
                if let Some(out) = child.stdout.take() {
                    for line in BufReader::new(out).lines() {
                        match line {
                            Ok(l) => push_log(l, hwnd_raw),
                            Err(_) => break,
                        }
                    }
                }
                let code = child.wait().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
                INBOX.lock().unwrap().install_exit = Some(code);
            }
            Err(e) => {
                push_log(format!("启动安装脚本失败：{e}"), hwnd_raw);
                INBOX.lock().unwrap().install_exit = Some(-1);
            }
        }
        post(hwnd_raw, WM_GUIDE_INSTALL_DONE, 0);
    });
}

fn start_verify(hwnd_raw: usize) {
    std::thread::spawn(move || {
        let mut code = 0u32;
        for _ in 0..40 {
            let list = display::list();
            if list.iter().any(|d| d.is_virtual() && d.is_phone_mode()) {
                code = 2;
                break;
            }
            if list.iter().any(|d| d.is_virtual()) {
                code = 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        INBOX.lock().unwrap().verify = Some(code);
        post(hwnd_raw, WM_GUIDE_VERIFIED, 0);
    });
}

fn goto_verify() {
    let phone_ok = GUIDE.with(|c| {
        let mut cell = c.borrow_mut();
        match cell.as_mut() {
            Some(s) => {
                s.step = Step::Verify;
                s.after_install = false;
                s.phone_ok
            }
            None => false,
        }
    });
    if phone_ok {
        finish_success();
        return;
    }
    set_text(
        "第 3 步（共 3 步）：创建手机屏并验证\n\n\
         驱动已经就绪。点下面的按钮，向导会让主程序创建一块手机尺寸的虚拟屏，\
         并确认它真的出现在桌面上。\n\n\
         创建时屏幕可能会闪一下，属正常现象。",
    );
    set_action("创建手机屏并验证", true);
}

fn finish_success() {
    GUIDE.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.step = Step::Done;
            s.busy = false;
        }
    });
    let dims = display::list()
        .into_iter()
        .find(|d| d.is_virtual() && d.is_phone_mode())
        .map(|d| format!("{}×{}", d.width, d.height))
        .unwrap_or_default();
    set_text(&format!(
        "全部搞通 ✓\n\n\
         虚拟屏已经创建并验证通过（{dims}）。\n\n\
         现在可以用手机打开向日葵，选这块虚拟屏连进来。\n\
         向导可以关了，之后日常使用不需要再走一遍。"
    ));
    set_action("完成", true);
}

fn on_checked() {
    let r = INBOX.lock().unwrap().check.take();
    let Some(r) = r else { return };
    let after_install = GUIDE.with(|c| {
        let mut cell = c.borrow_mut();
        match cell.as_mut() {
            Some(s) => {
                s.busy = false;
                s.device_ok = r.device.is_some();
                s.phone_ok = r.phone_now;
                s.after_install
            }
            None => false,
        }
    });
    if let Some(desc) = &r.device {
        append_line(&format!("✓ 驱动设备已找到：{desc}"));
        append_line(if r.phone_now {
            "✓ 虚拟屏已在桌面上，且是手机尺寸"
        } else if r.virtual_now {
            "· 虚拟屏已在桌面上（还不是手机尺寸，验证一步会处理）"
        } else {
            "· 虚拟屏还没出现（验证一步会创建）"
        });
        if after_install {
            append_line("\n安装成功，驱动已经装好了。");
        }
        set_action("下一步：验证虚拟屏", true);
    } else {
        append_line("✗ 没找到虚拟显示驱动设备（MttVDD）");
        if after_install {
            append_line(
                "\n安装跑完了，但还是没找到驱动设备。常见原因：安全软件拦截了驱动安装、\
                 系统拒绝了自签名驱动。可以打开安装日志看最后几行：\n\
                 %TEMP%\\..\\setup-virtual-display.log（在安装脚本旁边）\n\
                 把日志发给懂的人看，或点下面的按钮再装一次。",
            );
            set_action("重新安装", true);
        } else {
            append_line(
                "\n没有这个驱动就变不出虚拟屏。点「开始安装驱动」，向导会自动：\n\
                 1）导入驱动的签名证书（不导入系统会拒绝这个驱动）\n\
                 2）把驱动装进系统并创建虚拟屏设备\n\
                 过程约 1–3 分钟，屏幕可能会闪几下，属正常现象。",
            );
            set_action("开始安装驱动", true);
        }
    }
}

fn on_action(hwnd: HWND) {
    let (step, busy, device_ok) = GUIDE.with(|c| {
        let cell = c.borrow();
        match cell.as_ref() {
            Some(s) => (s.step, s.busy, s.device_ok),
            None => (Step::Check, true, false),
        }
    });
    if busy {
        return;
    }
    match step {
        Step::Done => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        Step::Verify => {
            GUIDE.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.busy = true;
                }
            });
            append_line("\n正在创建手机屏并验证（最长等 20 秒）…");
            set_action("正在验证…", false);
            crate::create_phone_screen();
            start_verify(hwnd.0 as usize);
        }
        Step::Check | Step::Install => {
            if device_ok {
                goto_verify();
            } else {
                GUIDE.with(|c| {
                    if let Some(s) = c.borrow_mut().as_mut() {
                        s.step = Step::Install;
                        s.busy = true;
                    }
                });
                set_text(
                    "第 2 步（共 3 步）：安装驱动\n\n正在安装，输出在下面实时滚动，不要关这个窗口…\n",
                );
                set_action("正在安装…", false);
                start_install(hwnd.0 as usize);
            }
        }
    }
}

unsafe extern "system" fn guide_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let dpi = unsafe { GetDeviceCaps(Some(GetDC(Some(hwnd))), LOGPIXELSX) };
            let sc = |v: i32| v * dpi / 96;
            let mut rect = RECT::default();
            let _ = unsafe { GetClientRect(hwnd, &mut rect) };
            let cw = rect.right - rect.left;
            let ch = rect.bottom - rect.top;
            let font_title = make_font(sc(-20), 600);
            let font_body = make_font(sc(-16), 400);
            let base = WS_CHILD | WS_VISIBLE;
            let title = crate::mk_control(
                hwnd,
                "Static",
                "配置向导",
                base,
                sc(20),
                sc(14),
                cw - sc(40),
                sc(28),
                3001,
                font_title,
            );
            let _ = title;
            let body = crate::mk_control(
                hwnd,
                "Edit",
                "",
                base | WS_BORDER | WS_VSCROLL
                    | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32),
                sc(20),
                sc(52),
                cw - sc(40),
                ch - sc(52) - sc(84),
                3004,
                font_body,
            );
            let action = crate::mk_control(
                hwnd,
                "Button",
                "正在检查…",
                base,
                sc(20),
                ch - sc(64),
                sc(280),
                sc(42),
                ID_ACTION,
                font_body,
            );
            let _ = crate::mk_control(
                hwnd,
                "Button",
                "关闭",
                base,
                cw - sc(170),
                ch - sc(64),
                sc(150),
                sc(42),
                ID_CLOSE,
                font_body,
            );
            set_enabled(action, false);
            GUIDE.with(|c| {
                *c.borrow_mut() = Some(GuideState {
                    hwnd,
                    body,
                    action,
                    step: Step::Check,
                    busy: true,
                    device_ok: false,
                    phone_ok: false,
                    after_install: false,
                    text: String::new(),
                });
            });
            set_text(
                "第 1 步（共 3 步）：检查驱动\n\n\
                 这个向导会一步步把虚拟屏搞通：先检查驱动在不在，缺了就装，\
                 最后创建一块手机屏验证。\n\n正在检查…",
            );
            start_check(hwnd.0 as usize);
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 as u32 & 0xffff) as i32;
            match id {
                ID_ACTION => on_action(hwnd),
                ID_CLOSE => unsafe {
                    let _ = DestroyWindow(hwnd);
                },
                _ => {}
            }
            LRESULT(0)
        }
        WM_GUIDE_CHECKED => {
            on_checked();
            LRESULT(0)
        }
        WM_GUIDE_LOG => {
            let lines: Vec<String> = {
                let mut inbox = INBOX.lock().unwrap();
                std::mem::take(&mut inbox.log)
            };
            for line in lines {
                append_line(&line);
            }
            LRESULT(0)
        }
        WM_GUIDE_INSTALL_DONE => {
            let exit = INBOX.lock().unwrap().install_exit.take().unwrap_or(-1);
            GUIDE.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.after_install = true;
                    s.busy = true;
                }
            });
            append_line(&format!(
                "\n安装脚本已结束（退出码 {exit}），重新检查驱动…"
            ));
            set_action("正在检查…", false);
            start_check(hwnd.0 as usize);
            LRESULT(0)
        }
        WM_GUIDE_VERIFIED => {
            let code = INBOX.lock().unwrap().verify.take().unwrap_or(0);
            GUIDE.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.busy = false;
                }
            });
            match code {
                2 => finish_success(),
                1 => {
                    append_line(
                        "\n虚拟屏出现了，但还不是手机尺寸。主窗口日志里有创建细节，\
                         也可以点「重试」再来一次。",
                    );
                    set_action("重试", true);
                }
                _ => {
                    append_line(
                        "\n等了 20 秒还没看到虚拟屏。主窗口的日志里有创建过程的细节，\
                         把主窗口切出来看最后几行；也可以点「重试」。",
                    );
                    set_action("重试", true);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            GUIDE.with(|c| {
                *c.borrow_mut() = None;
            });
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
