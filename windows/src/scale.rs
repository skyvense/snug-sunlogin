//! 单块显示器的显示缩放（DPI 缩放）读写：走 CCD 的未公开接口
//! DISPLAYCONFIG_DEVICE_INFO_GET/SET_SOURCE_DPI_SCALE（type -3 / -4），
//! 与 Firefox、SetDPI、Wine 测试同款用法：
//! - 缩放阶梯固定 12 档：100,125,150,175,200,225,250,300,350,400,450,500；
//! - GET 返回相对步数（min/cur/max），以 Windows 给这块屏的「推荐档」为 0 点；
//! - SET 写相对步数即可，即时生效，不用注销。
//! GET 结构体后三个 i32 的字段序有 (min,max,cur) 与 (min,cur,max) 两说，
//! 运行时用 min<=cur<=max 不变式判定（两序仅在 b==c 时都成立，而那时解读相同）。
//! 设置后一律读回该屏 DC 的有效 DPI 验证，没到位如实报错，不假报。

use crate::display::wide;
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Devices::Display::*;
use windows::Win32::Foundation::{LUID, WIN32_ERROR};
use windows::Win32::Graphics::Gdi::{CreateDCW, DeleteDC, GetDeviceCaps, LOGPIXELSX};

/// 手机屏缩放下限：建屏后至少抬到这个数（手机上界面大、好点）。
pub const PHONE_MIN_PERCENT: u32 = 250;

const SCALE_LADDER: [u32; 12] = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500];
const GET_SOURCE_DPI_SCALE: i32 = -3;
const SET_SOURCE_DPI_SCALE: i32 = -4;

#[repr(C)]
struct GetSourceDpiScale {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    fields: [i32; 3],
}

#[repr(C)]
struct SetSourceDpiScale {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    scale_rel: i32,
}

pub struct ScaleInfo {
    /// 当前有效缩放 %（由该屏 DC 的 LOGPIXELSX 换算）。
    pub percent: u32,
    /// 当前相对步数（相对推荐档）。
    pub rel: i32,
    pub min_rel: i32,
    pub max_rel: i32,
    /// Windows 给这块屏的推荐缩放 %。
    pub recommended_percent: u32,
}

fn zeroed<T>() -> T {
    unsafe { std::mem::zeroed() }
}

/// 在当前显示拓扑里按 GDI 设备名（\\.\DISPLAYn）找 CCD 源（adapter LUID + source id）。
fn find_source(device_name: &str) -> Result<(LUID, u32), String> {
    unsafe {
        let flags = QDC_ONLY_ACTIVE_PATHS;
        let (mut npaths, mut nmodes) = (0u32, 0u32);
        if GetDisplayConfigBufferSizes(flags, &mut npaths, &mut nmodes) != WIN32_ERROR(0) {
            return Err("读取显示拓扑尺寸失败".into());
        }
        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = Vec::new();
        let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = Vec::new();
        for _ in 0..npaths {
            paths.push(zeroed());
        }
        for _ in 0..nmodes {
            modes.push(zeroed());
        }
        let rc = QueryDisplayConfig(
            flags,
            &mut npaths,
            paths.as_mut_ptr(),
            &mut nmodes,
            modes.as_mut_ptr(),
            None,
        );
        if rc != WIN32_ERROR(0) {
            return Err(format!("读取显示拓扑失败（错误码 {}）", rc.0));
        }
        paths.truncate(npaths as usize);
        for p in &paths {
            let mut name: DISPLAYCONFIG_SOURCE_DEVICE_NAME = zeroed();
            name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            name.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            name.header.adapterId = p.sourceInfo.adapterId;
            name.header.id = p.sourceInfo.id;
            if DisplayConfigGetDeviceInfo(&mut name.header) == 0 {
                let raw = String::from_utf16_lossy(&name.viewGdiDeviceName);
                let gdi = raw.trim_end_matches('\0');
                if gdi.eq_ignore_ascii_case(device_name) {
                    return Ok((p.sourceInfo.adapterId, p.sourceInfo.id));
                }
            }
        }
        Err(format!("显示拓扑里找不到 {device_name}"))
    }
}

/// 这块屏当前的有效缩放 %（显示 DC 的 LOGPIXELSX：96dpi = 100%）。
pub fn percent_of(device_name: &str) -> Option<u32> {
    let dev = wide(device_name);
    unsafe {
        let hdc = CreateDCW(PCWSTR::null(), PCWSTR(dev.as_ptr()), PCWSTR::null(), None);
        if hdc.is_invalid() {
            return None;
        }
        let dpi = GetDeviceCaps(Some(hdc), LOGPIXELSX);
        let _ = DeleteDC(hdc);
        if dpi <= 0 {
            None
        } else {
            Some((dpi as u32 * 100 + 48) / 96)
        }
    }
}

fn ladder_index(percent: u32) -> usize {
    SCALE_LADDER
        .iter()
        .enumerate()
        .min_by_key(|(_, &p)| (p as i32 - percent as i32).abs())
        .map(|(i, _)| i)
        .unwrap_or(0)
}

pub fn query(device_name: &str) -> Result<ScaleInfo, String> {
    let (adapter, id) = find_source(device_name)?;
    let mut req: GetSourceDpiScale = zeroed();
    req.header.r#type = DISPLAYCONFIG_DEVICE_INFO_TYPE(GET_SOURCE_DPI_SCALE);
    req.header.size = std::mem::size_of::<GetSourceDpiScale>() as u32;
    req.header.adapterId = adapter;
    req.header.id = id;
    let rc = unsafe { DisplayConfigGetDeviceInfo(&mut req.header) };
    if rc != 0 {
        return Err(format!("读取缩放步数失败（返回 {rc}）"));
    }
    let [a, b, c] = req.fields;
    let (min_rel, max_rel, rel) = if a <= c && c <= b {
        (a, b, c) // (min, max, cur)
    } else if a <= b && b <= c {
        (a, c, b) // (min, cur, max)
    } else {
        return Err(format!("缩放步数读数不合常理（{a}, {b}, {c}），不敢动"));
    };
    let percent = percent_of(device_name).ok_or("读不到这块屏的有效 DPI")?;
    let rec_idx = (ladder_index(percent) as i32 - rel).clamp(0, SCALE_LADDER.len() as i32 - 1);
    Ok(ScaleInfo {
        percent,
        rel,
        min_rel,
        max_rel,
        recommended_percent: SCALE_LADDER[rec_idx as usize],
    })
}

/// 直接按相对步数设置（还原用：写回建屏前记下的步数）。
pub fn set_rel(device_name: &str, rel: i32) -> Result<(), String> {
    let (adapter, id) = find_source(device_name)?;
    let mut req: SetSourceDpiScale = zeroed();
    req.header.r#type = DISPLAYCONFIG_DEVICE_INFO_TYPE(SET_SOURCE_DPI_SCALE);
    req.header.size = std::mem::size_of::<SetSourceDpiScale>() as u32;
    req.header.adapterId = adapter;
    req.header.id = id;
    req.scale_rel = rel;
    let rc = unsafe { DisplayConfigSetDeviceInfo(&req.header) };
    if rc != 0 {
        return Err(format!("设置缩放步数失败（返回 {rc}）"));
    }
    Ok(())
}

/// 把缩放设到目标 %（钳在系统允许的步数范围内），等生效后读回实际 % 返回。
pub fn set_percent(device_name: &str, target: u32) -> Result<u32, String> {
    let info = query(device_name)?;
    let rec_idx = ladder_index(info.recommended_percent) as i32;
    let target_idx = (ladder_index(target) as i32)
        .clamp(rec_idx + info.min_rel, rec_idx + info.max_rel);
    set_rel(device_name, target_idx - rec_idx)?;
    std::thread::sleep(Duration::from_millis(400));
    percent_of(device_name).ok_or_else(|| "设置后读不回有效 DPI".to_string())
}
