//! 虚拟屏串流测试窗口：铺满目标显示器，提供「持续在动」和「完全静止」两种画面，
//! 对照 mac 版未结的卡顿问题——
//! 动态流畅、静态冻住 → 向日葵对低变化内容跳帧/缓冲；两种都卡 → 恒定的串流赤字。
//! 帧号大字显示，手机端一眼能看出画面有没有在动、动得匀不匀。

use crate::display::wide;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS_NAME: &str = "SnugSunloginTestWindow";
const ID_TOGGLE: i32 = 2001;
const ID_CLOSE: i32 = 2002;
const TIMER_ID: usize = 7;
const BALL_R: i32 = 90;

struct TestState {
    hwnd: HWND,
    btn_toggle: HWND,
    font_big: HFONT,
    font_mid: HFONT,
    frame: u64,
    dynamic: bool,
    bx: i32,
    by: i32,
    vx: i32,
    vy: i32,
    w: i32,
    h: i32,
}

thread_local! {
    static TEST: RefCell<Option<TestState>> = const { RefCell::new(None) };
}

static REGISTERED: AtomicBool = AtomicBool::new(false);

fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | (g << 8) | (b << 16))
}

/// TextOutW 用：不带结尾 NUL 的 UTF-16（长度由切片自带）。
fn text16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// 简单的 HSV→RGB（h: 0..360），给动态背景一个缓慢流转的颜色。
fn hsv(h: u32, s: f32, v: f32) -> COLORREF {
    let c = v * s;
    let x = c * (1.0 - ((h as f32 / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match h / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    rgb(
        ((r + m) * 255.0) as u32,
        ((g + m) * 255.0) as u32,
        ((b + m) * 255.0) as u32,
    )
}

fn make_font(height: i32, weight: i32) -> HFONT {
    let mut lf = LOGFONTW::default();
    lf.lfHeight = height;
    lf.lfWeight = weight;
    let face = wide("Segoe UI");
    lf.lfFaceName[..face.len() - 1].copy_from_slice(&face[..face.len() - 1]);
    unsafe { CreateFontIndirectW(&lf) }
}

pub fn is_open() -> bool {
    TEST.with(|c| c.borrow().is_some())
}

pub fn close() {
    let hwnd = TEST.with(|c| c.borrow().as_ref().map(|s| s.hwnd));
    if let Some(hwnd) = hwnd {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// 在 (x, y, w, h) 这块显示器的区域上铺满测试窗口。
pub fn open(x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
    if is_open() {
        return Ok(());
    }
    let hinstance = unsafe { GetModuleHandleW(None) }.map_err(|e| format!("GetModuleHandleW 失败：{e}"))?;
    let hinstance = HINSTANCE(hinstance.0);
    if !REGISTERED.swap(true, Ordering::SeqCst) {
        let class_w = wide(CLASS_NAME);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(test_wnd_proc),
            hInstance: hinstance,
            lpszClassName: PCWSTR(class_w.as_ptr()),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            ..Default::default()
        };
        let atom = unsafe { RegisterClassW(&wc) };
        if atom == 0 {
            REGISTERED.store(false, Ordering::SeqCst);
            return Err("RegisterClassW 失败".to_string());
        }
    }
    TEST.with(|c| {
        *c.borrow_mut() = Some(TestState {
            hwnd: HWND::default(),
            btn_toggle: HWND::default(),
            font_big: make_font(-150, 700),
            font_mid: make_font(-44, 400),
            frame: 0,
            dynamic: true,
            bx: w / 2,
            by: h / 2,
            vx: 5,
            vy: 4,
            w,
            h,
        });
    });
    let class_w = wide(CLASS_NAME);
    let title_w = wide("Snug 串流测试");
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(hinstance),
            None,
        )
    }
    .map_err(|e| format!("CreateWindowExW 失败：{e}"))?;
    TEST.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            s.hwnd = hwnd;
        }
    });
    Ok(())
}

fn toggle_dynamic() {
    let (btn, dynamic) = TEST.with(|c| {
        let mut cell = c.borrow_mut();
        match cell.as_mut() {
            Some(s) => {
                s.dynamic = !s.dynamic;
                (s.btn_toggle, s.dynamic)
            }
            None => (HWND::default(), false),
        }
    });
    if !btn.is_invalid() {
        let text = wide(if dynamic { "切到静态" } else { "切到动态" });
        unsafe {
            let _ = SetWindowTextW(btn, PCWSTR(text.as_ptr()));
        }
    }
}

fn paint(hwnd: HWND) {
    TEST.with(|c| {
        let cell = c.borrow();
        let Some(s) = cell.as_ref() else { return };
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
            if w > 0 && h > 0 {
                // 双缓冲：先画到内存 DC 再整幅贴上，避免闪烁干扰串流判断
                let mem = CreateCompatibleDC(Some(hdc));
                let bmp = CreateCompatibleBitmap(hdc, w, h);
                let old_bmp = SelectObject(mem, HGDIOBJ(bmp.0));

                let bg = if s.dynamic {
                    hsv((s.frame % 360) as u32, 0.45, 0.45)
                } else {
                    rgb(35, 42, 52)
                };
                let brush = CreateSolidBrush(bg);
                FillRect(mem, &rc, brush);
                let _ = DeleteObject(HGDIOBJ(brush.0));

                // 扫过的竖条：匀速移动，卡顿一眼可见
                let bar_x = ((s.frame * 11) % (w as u64 + 240)) as i32 - 120;
                let bar = CreateSolidBrush(rgb(200, 210, 220));
                let old = SelectObject(mem, HGDIOBJ(bar.0));
                let _ = Rectangle(mem, bar_x, 0, bar_x + 70, h);
                SelectObject(mem, old);
                let _ = DeleteObject(HGDIOBJ(bar.0));

                // 弹跳的球（品牌珊瑚色，与 mac 版图标呼应）
                let ball = CreateSolidBrush(rgb(243, 107, 83));
                let old = SelectObject(mem, HGDIOBJ(ball.0));
                let _ = Ellipse(mem, s.bx - BALL_R, s.by - BALL_R, s.bx + BALL_R, s.by + BALL_R);
                SelectObject(mem, old);
                let _ = DeleteObject(HGDIOBJ(ball.0));

                SetBkMode(mem, TRANSPARENT);
                SetTextColor(mem, rgb(255, 255, 255));
                let old_font = SelectObject(mem, HGDIOBJ(s.font_big.0));
                let text = text16(&format!("FRAME {}", s.frame));
                let _ = TextOutW(mem, 36, h / 2 - 200, &text);
                SelectObject(mem, HGDIOBJ(s.font_mid.0));
                let mode = if s.dynamic {
                    "DYNAMIC · 60 FPS · 帧号持续在变"
                } else {
                    "STATIC · 已冻结 · 画面不再变化"
                };
                let text = text16(mode);
                let _ = TextOutW(mem, 36, h / 2 + 10, &text);
                let t = GetLocalTime();
                let text = text16(&format!(
                    "{:02}:{:02}:{:02}.{:03}",
                    t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
                ));
                let _ = TextOutW(mem, 36, h / 2 + 80, &text);
                let text = text16("Esc 或点「关闭」退出 · 左上角按钮切换动/静");
                let _ = TextOutW(mem, 36, h - 70, &text);
                SelectObject(mem, old_font);

                let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
                SelectObject(mem, old_bmp);
                let _ = DeleteObject(HGDIOBJ(bmp.0));
                let _ = DeleteDC(mem);
            }
            let _ = EndPaint(hwnd, &ps);
        }
    });
}

unsafe extern "system" fn test_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let hinstance = HINSTANCE(
                unsafe { GetModuleHandleW(None) }.map(|m| m.0).unwrap_or_default(),
            );
            let mk = |text: &str, id: i32, x: i32| -> HWND {
                let class_w = wide("Button");
                let text_w = wide(text);
                unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        PCWSTR(class_w.as_ptr()),
                        PCWSTR(text_w.as_ptr()),
                        WS_CHILD | WS_VISIBLE,
                        x,
                        16,
                        132,
                        40,
                        Some(hwnd),
                        Some(HMENU(id as *mut core::ffi::c_void)),
                        Some(hinstance),
                        None,
                    )
                }
                .unwrap_or_default()
            };
            let btn_toggle = mk("切到静态", ID_TOGGLE, 16);
            let _btn_close = mk("关闭", ID_CLOSE, 160);
            TEST.with(|c| {
                if let Some(s) = c.borrow_mut().as_mut() {
                    s.btn_toggle = btn_toggle;
                }
            });
            unsafe {
                SetTimer(Some(hwnd), TIMER_ID, 16, None);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_ID {
                TEST.with(|c| {
                    if let Some(s) = c.borrow_mut().as_mut() {
                        if s.dynamic {
                            s.frame += 1;
                            s.bx += s.vx;
                            s.by += s.vy;
                            if s.bx < BALL_R || s.bx > s.w - BALL_R {
                                s.vx = -s.vx;
                                s.bx += s.vx;
                            }
                            if s.by < BALL_R || s.by > s.h - BALL_R {
                                s.vy = -s.vy;
                                s.by += s.vy;
                            }
                        }
                    }
                });
                let dynamic = TEST.with(|c| c.borrow().as_ref().map(|s| s.dynamic).unwrap_or(false));
                if dynamic {
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 as u32 & 0xffff) as i32;
            if id == ID_TOGGLE {
                toggle_dynamic();
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            } else if id == ID_CLOSE {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 as u32 == 0x1B {
                // VK_ESCAPE
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_ID);
            }
            TEST.with(|c| {
                if let Some(s) = c.borrow_mut().take() {
                    unsafe {
                        let _ = DeleteObject(HGDIOBJ(s.font_big.0));
                        let _ = DeleteObject(HGDIOBJ(s.font_mid.0));
                    }
                }
            });
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
