//! 显示器枚举与模式切换。只用 user32 的经典 API（EnumDisplayDevices /
//! EnumDisplaySettings / ChangeDisplaySettingsEx），任何语言区域设置下结果一致，
//! 不碰驱动私有接口：虚拟屏是 VDD 驱动提供的普通显示器。

use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::*;

// 手机屏规格不在这里写死：见 phones（按所选型号缩放，默认 1080×2346@60）。

// DISPLAY_DEVICE.StateFlags 的位（用字面量避免类型摩擦，含义见 Win32 文档）
const ATTACHED_TO_DESKTOP: u32 = 0x1;
const PRIMARY_DEVICE: u32 = 0x4;

/// 一块当前接着的显示器（逻辑信息）。
#[derive(Clone)]
pub struct DisplayInfo {
    pub device_name: String, // \\.\DISPLAYn，设模式时用
    pub adapter_name: String,
    pub monitor_name: String,
    pub primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub hz: u32,
}

impl DisplayInfo {
    /// 是不是当前所选型号的手机屏模式（规格在 phones 里，按型号缩放）。
    pub fn is_phone_mode(&self) -> bool {
        let (w, h, _) = crate::phones::current_spec();
        self.width == w && self.height == h
    }
    /// 是不是 VDD 的虚拟屏：适配器名在不同 VDD 版本里有两种叫法。
    pub fn is_virtual(&self) -> bool {
        self.adapter_name.contains("Virtual Display")
            || self.adapter_name.contains("IddSample")
    }
    pub fn describe(&self) -> String {
        let mut s = format!(
            "{} · {} · {}×{}@{} · ({},{})",
            self.device_name, self.adapter_name, self.width, self.height, self.hz, self.x, self.y
        );
        if self.primary {
            s += " · 主屏";
        }
        if self.is_virtual() {
            s += " · VDD 虚拟屏";
        }
        if self.is_phone_mode() {
            s += " · 手机屏模式";
        }
        if !self.monitor_name.is_empty() {
            s += &format!(" · {}", self.monitor_name);
        }
        s
    }
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// 枚举当前接到桌面的显示器。
pub fn list() -> Vec<DisplayInfo> {
    let mut out = Vec::new();
    let mut i = 0u32;
    loop {
        let mut dd = DISPLAY_DEVICEW::default();
        dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        let ok = unsafe { EnumDisplayDevicesW(PCWSTR::null(), i, &mut dd, 0) };
        if !ok.as_bool() {
            break;
        }
        i += 1;
        if dd.StateFlags.0 & ATTACHED_TO_DESKTOP == 0 {
            continue;
        }
        let device_name = from_wide(&dd.DeviceName);
        let name_w = wide(&device_name);

        let mut mon = DISPLAY_DEVICEW::default();
        mon.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        let monitor_name =
            if unsafe { EnumDisplayDevicesW(PCWSTR(name_w.as_ptr()), 0, &mut mon, 0) }.as_bool() {
                from_wide(&mon.DeviceString)
            } else {
                String::new()
            };

        let mut dm = DEVMODEW::default();
        dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        let ok = unsafe { EnumDisplaySettingsW(PCWSTR(name_w.as_ptr()), ENUM_CURRENT_SETTINGS, &mut dm) };
        if !ok.as_bool() {
            continue;
        }
        let pos = unsafe { dm.Anonymous1.Anonymous2.dmPosition };
        out.push(DisplayInfo {
            device_name,
            adapter_name: from_wide(&dd.DeviceString),
            monitor_name,
            primary: dd.StateFlags.0 & PRIMARY_DEVICE != 0,
            x: pos.x,
            y: pos.y,
            width: dm.dmPelsWidth,
            height: dm.dmPelsHeight,
            hz: dm.dmDisplayFrequency,
        });
    }
    out
}

/// 把指定显示器设成 w×h@hz 并写入注册表（重启后保留）。
/// 返回 ChangeDisplaySettingsEx 的原始返回码：0 = DISP_CHANGE_SUCCESSFUL。
pub fn set_mode(device_name: &str, w: u32, h: u32, hz: u32) -> i32 {
    let name_w = wide(device_name);
    let mut dm = DEVMODEW::default();
    dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    dm.dmPelsWidth = w;
    dm.dmPelsHeight = h;
    dm.dmDisplayFrequency = hz;
    dm.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY;
    let r = unsafe {
        ChangeDisplaySettingsExW(
            PCWSTR(name_w.as_ptr()),
            Some(&dm),
            None,
            CDS_UPDATEREGISTRY,
            None,
        )
    };
    // 再调一次空参数把挂起的模式变更真正应用（Win32 的标准两步法）
    unsafe {
        ChangeDisplaySettingsExW(PCWSTR::null(), None, None, CDS_TYPE(0), None);
    }
    r.0
}
