//! 配置向导：驱动没装、虚拟屏用不了时，一步步带用户搞通——
//! 检查驱动 → 安装驱动（跑随包的 setup-virtual-display.ps1，输出实时进窗口）→
//! 创建手机屏并验证它真的出现。主程序带 --guide 启动时直接打开；主窗口的
//! 「配置向导」按钮、以及启动时检测到没驱动的询问框也能打开它。

use crate::devices;
use crate::display::{self, wide};
use crate::lang::tr;
use crate::tr_fmt;
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
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS_NAME: &str = "SnugSunloginGuideWindow";
const ID_ACTION: i32 = 3002;
const ID_CLOSE: i32 = 3003;

const WM_GUIDE_CHECKED: u32 = WM_APP + 31;
const WM_GUIDE_LOG: u32 = WM_APP + 32;
const WM_GUIDE_INSTALL_DONE: u32 = WM_APP + 33;
const WM_GUIDE_VERIFIED: u32 = WM_APP + 34;
const WM_GUIDE_REMOVED: u32 = WM_APP + 35;

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Check,
    Install,
    Verify,
    Remove,
    Test,
    Done,
}

/// 创建验证用在两处：第 3 步创建完要接着走删除，第 5 步创建完要开测试画面
#[derive(Clone, Copy, PartialEq)]
enum VerifyFor {
    Create,
    Test,
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
    removed: Option<bool>,
}

static INBOX: Mutex<Inbox> = Mutex::new(Inbox {
    check: None,
    log: Vec::new(),
    install_exit: None,
    verify: None,
    removed: None,
});

struct GuideState {
    hwnd: HWND,
    body: HWND,
    action: HWND,
    step: Step,
    busy: bool,
    device_ok: bool,
    verify_for: VerifyFor,
    after_install: bool,
    text: String,
    /// 动作按钮是否在闪烁（可点状态的提醒），以及当前闪到哪一态
    flashing: bool,
    flash_phase: bool,
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
        let title_w = wide(tr("配置向导 — 把虚拟屏一步步搞通", "Setup wizard — get the virtual screen working step by step"));
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

/// 启用/禁用按钮。EnableWindow 在 windows 0.59 里挂在 Input::KeyboardAndMouse 下，
/// 不在 WindowsAndMessaging——之前改 WS_DISABLED 样式位的做法不触发按钮重绘，
/// 按钮亮不起来，必须走这个正道。
fn set_enabled(hwnd: HWND, on: bool) {
    unsafe {
        let _ = EnableWindow(hwnd, on);
    }
}

const TIMER_FLASH: usize = 9;

/// 按钮闪烁：可点状态下用定时器翻转 BS_DEFPUSHBUTTON（粗边框），
/// 一亮一暗地提醒「现在点我」。点下或变忙即停。
fn set_flash(on: bool) {
    let (hwnd, action) = GUIDE.with(|c| {
        let mut cell = c.borrow_mut();
        match cell.as_mut() {
            Some(s) => {
                s.flashing = on;
                s.flash_phase = false;
                (Some(s.hwnd), Some(s.action))
            }
            None => (None, None),
        }
    });
    let (Some(hwnd), Some(action)) = (hwnd, action) else {
        return;
    };
    unsafe {
        if on {
            SetTimer(Some(hwnd), TIMER_FLASH, 550, None);
        } else {
            let _ = KillTimer(Some(hwnd), TIMER_FLASH);
            set_flash_phase(action, 0);
        }
    }
}

/// 闪烁相位写进按钮的 GWLP_USERDATA（自绘按钮 draw_button 读它决定深浅/粗边），
/// 再让按钮重绘一帧。按钮改自绘后不能再靠 BS_DEFPUSHBUTTON 样式位闪了。
fn set_flash_phase(btn: HWND, phase: i32) {
    unsafe {
        SetWindowLongW(btn, GWLP_USERDATA, phase);
        let _ = InvalidateRect(Some(btn), None, false);
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
        // 能点了就闪，忙起来就停
        set_flash(enabled);
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
            .map(|d| tr_fmt!("{}（{}）", "{} ({})", d.friendly_name, d.instance_id)),
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
                tr("exe 旁边没找到 setup-virtual-display.ps1：它与 exe 在同一个发布包里，确认解压完整", "setup-virtual-display.ps1 not found next to the exe: it ships in the same release package; make sure the extraction is complete").to_string(),
                hwnd_raw,
            );
            INBOX.lock().unwrap().install_exit = Some(-1);
            post(hwnd_raw, WM_GUIDE_INSTALL_DONE, 0);
            return;
        }
        push_log(tr_fmt!("开始运行 {}", "Running {}", script.display()), hwnd_raw);
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
                push_log(tr_fmt!("启动安装脚本失败：{e}", "Failed to start the install script: {e}"), hwnd_raw);
                INBOX.lock().unwrap().install_exit = Some(-1);
            }
        }
        post(hwnd_raw, WM_GUIDE_INSTALL_DONE, 0);
    });
}

fn start_remove_poll(hwnd_raw: usize) {
    std::thread::spawn(move || {
        let mut removed = false;
        for _ in 0..40 {
            if !display::list().iter().any(|d| d.is_virtual()) {
                removed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        INBOX.lock().unwrap().removed = Some(removed);
        post(hwnd_raw, WM_GUIDE_REMOVED, 0);
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

fn goto_create() {
    GUIDE.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.step = Step::Verify;
            s.verify_for = VerifyFor::Create;
            s.after_install = false;
        }
    });
    set_text(
        tr("第 3 步（共 5 步）：创建手机屏并验证\n\n\
         驱动已经就绪。点下面的按钮，向导会让主程序创建一块手机尺寸的虚拟屏，\
         并确认它真的出现在桌面上。\n\n\
         创建时屏幕可能会闪一下，属正常现象。", "Step 3 of 5: Create the phone screen and verify\n\nThe driver is ready. Click the button below and the wizard will have the main app create a phone-sized virtual screen and confirm it really appears on the desktop.\n\nThe screen may flicker briefly while creating; that is normal."),
    );
    set_action(tr("创建手机屏并验证", "Create phone screen and verify"), true);
}

fn goto_remove() {
    GUIDE.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.step = Step::Remove;
        }
    });
    let dims = display::list()
        .into_iter()
        .find(|d| d.is_virtual() && d.is_phone_mode())
        .map(|d| format!("{}×{}", d.width, d.height))
        .unwrap_or_default();
    set_text(&tr_fmt!("第 4 步（共 5 步）：删除手机屏并验证\n\n\
         ✓ 手机屏已创建并验证通过（{dims}）。\n\n\
         接着验证删除：点下面的按钮，向导会把这块屏删掉，并确认它真的从桌面上消失。\
         删不掉的话自动模式断开时也拆不掉，所以这一步必须过。", "Step 4 of 5: Remove the phone screen and verify\n\n✓ The phone screen was created and verified ({dims}).\n\nNext, verify removal: click the button below and the wizard will remove this screen and confirm it really disappears from the desktop. If removal fails here, auto mode cannot remove it on disconnect either, so this step must pass."));
    set_action(tr("删除手机屏并验证", "Remove phone screen and verify"), true);
}

fn goto_test() {
    GUIDE.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.step = Step::Test;
            s.verify_for = VerifyFor::Test;
        }
    });
    set_text(
        tr("第 5 步（共 5 步）：打开测试画面\n\n\
         ✓ 删除已验证通过。\n\n\
         最后一步：重新创建手机屏，并把串流测试画面铺满它。\
         点下面的按钮之后，用手机打开向日葵连进这块虚拟屏，\
         看动画顺不顺、帧号跳不跳——顺了就算全程搞通。", "Step 5 of 5: Open the test pattern\n\n✓ Removal verified.\n\nFinal step: recreate the phone screen and fill it with the streaming test pattern. After clicking the button below, open Sunlogin on your phone, connect to this virtual screen, and check whether the animation runs smoothly and the frame counter advances without jumps — if it does, everything works end to end."),
    );
    set_action(tr("创建并打开测试画面", "Create and open the test pattern"), true);
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
                s.after_install
            }
            None => false,
        }
    });
    if let Some(desc) = &r.device {
        append_line(&tr_fmt!("✓ 驱动设备已找到：{desc}", "✓ Driver device found: {desc}"));
        append_line(if r.phone_now {
            tr("✓ 虚拟屏已在桌面上，且是手机尺寸", "✓ The virtual screen is on the desktop, at phone size")
        } else if r.virtual_now {
            tr("· 虚拟屏已在桌面上（还不是手机尺寸，验证一步会处理）", "· The virtual screen is on the desktop (not phone size yet; the verify step will handle it)")
        } else {
            tr("· 虚拟屏还没出现（验证一步会创建）", "· The virtual screen has not appeared yet (the verify step will create it)")
        });
        if after_install {
            append_line(tr("\n安装成功，驱动已经装好了。", "\nInstall succeeded; the driver is in place."));
        }
        set_action(tr("下一步：创建手机屏", "Next: create the phone screen"), true);
    } else {
        append_line(tr("✗ 没找到虚拟显示驱动设备（MttVDD）", "✗ Virtual display driver device (MttVDD) not found"));
        if after_install {
            append_line(
                tr("\n安装跑完了，但还是没找到驱动设备。常见原因：安全软件拦截了驱动安装、\
                 系统拒绝了自签名驱动。可以打开安装日志看最后几行：\n\
                 %TEMP%\\..\\setup-virtual-display.log（在安装脚本旁边）\n\
                 把日志发给懂的人看，或点下面的按钮再装一次。", "\nThe install finished, but the driver device is still not found. Common causes: security software blocked the driver installation, or the system rejected the self-signed driver. You can open the install log and read the last few lines:\n%TEMP%\\..\\setup-virtual-display.log (next to the install script)\nShow the log to someone technical, or click the button below to install again."),
            );
            set_action(tr("重新安装", "Reinstall"), true);
        } else {
            append_line(
                tr("\n没有这个驱动就变不出虚拟屏。点「开始安装驱动」，向导会自动：\n\
                 1）导入驱动的签名证书（不导入系统会拒绝这个驱动）\n\
                 2）把驱动装进系统并创建虚拟屏设备\n\
                 过程约 1–3 分钟，屏幕可能会闪几下，属正常现象。", "\nWithout this driver there is no virtual screen. Click \"Install driver\" and the wizard will automatically:\n1) Import the driver's signing certificate (without it the system rejects the driver)\n2) Install the driver into the system and create the virtual screen device\nThe process takes about 1–3 minutes; the screen may flicker a few times, which is normal."),
            );
            set_action(tr("开始安装驱动", "Install driver"), true);
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
                    s.verify_for = VerifyFor::Create;
                }
            });
            append_line(tr("\n正在创建手机屏并验证（最长等 20 秒）…", "\nCreating the phone screen and verifying (waiting up to 20 s)…"));
            set_action(tr("正在验证…", "Verifying…"), false);
            crate::create_phone_screen();
            crate::adopt_created_screen();
            start_verify(hwnd.0 as usize);
        }
        Step::Remove => {
            GUIDE.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.busy = true;
                }
            });
            append_line(tr("\n正在删除手机屏并验证（最长等 20 秒）…", "\nRemoving the phone screen and verifying (waiting up to 20 s)…"));
            set_action(tr("正在验证…", "Verifying…"), false);
            crate::remove_phone_screen();
            start_remove_poll(hwnd.0 as usize);
        }
        Step::Test => {
            GUIDE.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.busy = true;
                    s.verify_for = VerifyFor::Test;
                }
            });
            append_line(tr("\n正在创建手机屏并打开测试画面（最长等 20 秒）…", "\nCreating the phone screen and opening the test pattern (waiting up to 20 s)…"));
            set_action(tr("正在验证…", "Verifying…"), false);
            crate::create_phone_screen();
            crate::adopt_created_screen();
            start_verify(hwnd.0 as usize);
        }
        Step::Check | Step::Install => {
            if device_ok {
                goto_create();
            } else {
                GUIDE.with(|c| {
                    if let Some(s) = c.borrow_mut().as_mut() {
                        s.step = Step::Install;
                        s.busy = true;
                    }
                });
                set_text(
                    tr("第 2 步（共 5 步）：安装驱动\n\n正在安装，输出在下面实时滚动，不要关这个窗口…\n", "Step 2 of 5: Install the driver\n\nInstalling; the output scrolls below in real time. Do not close this window…\n"),
                );
                set_action(tr("正在安装…", "Installing…"), false);
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
                tr("配置向导", "Setup Wizard"),
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
                tr("正在检查…", "Checking…"),
                base | WINDOW_STYLE(0x0000_000B), // BS_OWNERDRAW：与主窗口同款自绘
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
                tr("关闭", "Close"),
                base | WINDOW_STYLE(0x0000_000B), // BS_OWNERDRAW
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
                    verify_for: VerifyFor::Create,
                    after_install: false,
                    text: String::new(),
                    flashing: false,
                    flash_phase: false,
                });
            });
            set_text(
                tr("第 1 步（共 5 步）：检查驱动\n\n\
                 这个向导会一步步把虚拟屏搞通：检查驱动 → 缺了就装 → \
                 创建、删除各验证一遍 → 最后打开测试画面看串流。\n\n正在检查…", "Step 1 of 5: Check the driver\n\nThis wizard gets the virtual screen working step by step: check the driver → install it if missing → verify creating and removing → finally open the test pattern to check streaming.\n\nChecking…"),
            );
            start_check(hwnd.0 as usize);
            LRESULT(0)
        }
        WM_DRAWITEM => {
            let dis = unsafe {
                &*(lparam.0 as *const windows::Win32::UI::Controls::DRAWITEMSTRUCT)
            };
            crate::draw_button(dis);
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC => {
            // 向导里唯一的静态控件是标题：深色字、白底（与主窗口主题一致）
            let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
            unsafe {
                SetTextColor(hdc, crate::rgb(29, 29, 31));
                SetBkMode(hdc, TRANSPARENT);
            }
            LRESULT(crate::card_brush().0 as isize)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_FLASH {
                let action = GUIDE.with(|c| {
                    let mut cell = c.borrow_mut();
                    match cell.as_mut() {
                        Some(s) if s.flashing => {
                            s.flash_phase = !s.flash_phase;
                            Some((s.action, s.flash_phase))
                        }
                        _ => None,
                    }
                });
                if let Some((action, phase)) = action {
                    set_flash_phase(action, if phase { 1 } else { 0 });
                }
            }
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
            append_line(&tr_fmt!("\n安装脚本已结束（退出码 {exit}），重新检查驱动…", "\nThe install script has finished (exit code {exit}); rechecking the driver…"));
            set_action(tr("正在检查…", "Checking…"), false);
            start_check(hwnd.0 as usize);
            LRESULT(0)
        }
        WM_GUIDE_VERIFIED => {
            let code = INBOX.lock().unwrap().verify.take().unwrap_or(0);
            let verify_for = GUIDE.with(|c| {
                let mut cell = c.borrow_mut();
                match cell.as_mut() {
                    Some(s) => {
                        s.busy = false;
                        s.verify_for
                    }
                    None => VerifyFor::Create,
                }
            });
            match (code, verify_for) {
                (2, VerifyFor::Create) => goto_remove(),
                (2, VerifyFor::Test) => {
                    // 测试画面铺到虚拟屏上，向导收官，屏留着给用户拿手机看
                    let target = display::list()
                        .into_iter()
                        .find(|d| d.is_virtual() && d.is_phone_mode());
                    GUIDE.with(|c| {
                        if let Some(s) = c.borrow_mut().as_mut() {
                            s.step = Step::Done;
                        }
                    });
                    match target {
                        Some(d) => {
                            match crate::testwin::open(
                                d.x,
                                d.y,
                                d.width as i32,
                                d.height as i32,
                            ) {
                                Ok(()) => set_text(&tr_fmt!("全部搞通 ✓\n\n\
                                     驱动安装、创建、删除都验证过了，测试画面已经铺在虚拟屏上（{}×{}）。\n\n\
                                     现在用手机打开向日葵连进这块屏，看动画顺不顺、帧号跳不跳。\
                                     测试画面按 Esc 或点它上面的「关闭」退出。\n\
                                     向导可以关了，之后日常使用不需要再走一遍。", "All done ✓\n\nDriver installation, creation and removal are all verified, and the test pattern is now on the virtual screen ({}×{}).\n\nNow open Sunlogin on your phone, connect to this screen, and check whether the animation runs smoothly and the frame counter advances without jumps.\nThe test pattern exits with Esc or its \"Close\" button.\nYou can close the wizard now; daily use does not need it again.",
                                    d.width, d.height)),
                                Err(e) => set_text(&tr_fmt!("创建和删除都验证过了，但测试画面没打开：{e}\n\n\
                                     可以回主窗口点「测试窗口」手动开。向导可以关了。", "Creation and removal are verified, but the test pattern did not open: {e}\n\nYou can open it manually from the main window via \"Test window\". You can close the wizard now.")),
                            }
                        }
                        None => set_text(
                            tr("创建和删除都验证过了，但回头没找到手机屏。\
                             回主窗口看一眼状态。向导可以关了。", "Creation and removal are verified, but the phone screen is nowhere to be found now.\nCheck the status in the main window. You can close the wizard now."),
                        ),
                    }
                    set_action(tr("完成", "Done"), true);
                }
                (1, _) => {
                    append_line(
                        tr("\n虚拟屏出现了，但还不是手机尺寸。主窗口日志里有创建细节，\
                         也可以点「重试」再来一次。", "\nThe virtual screen appeared, but it is not phone size yet. The main window's log has the creation details; you can also click \"Retry\" to try again."),
                    );
                    set_action(tr("重试", "Retry"), true);
                }
                _ => {
                    append_line(
                        tr("\n等了 20 秒还没看到虚拟屏。主窗口的日志里有创建过程的细节，\
                         把主窗口切出来看最后几行；也可以点「重试」。", "\nWaited 20 s and still no virtual screen. The main window's log has the creation details; bring it up and read the last few lines, or click \"Retry\"."),
                    );
                    set_action(tr("重试", "Retry"), true);
                }
            }
            LRESULT(0)
        }
        WM_GUIDE_REMOVED => {
            let removed = INBOX.lock().unwrap().removed.take().unwrap_or(false);
            GUIDE.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.busy = false;
                }
            });
            if removed {
                goto_test();
            } else {
                append_line(
                    tr("\n等了 20 秒虚拟屏还在桌面上，删除没验证过。主窗口日志里有删除过程的细节；\
                     也可以点「重试」。", "\nWaited 20 s and the virtual screen is still on the desktop; removal is not verified. The main window's log has the removal details; or click \"Retry\"."),
                );
                set_action(tr("重试", "Retry"), true);
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
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_FLASH);
            }
            GUIDE.with(|c| {
                *c.borrow_mut() = None;
            });
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
