//! 找虚拟显示驱动的 PnP 适配器设备、重启它让新配置生效。
//! 枚举走 PowerShell 的 Get-PnpDevice 并拼成竖线分隔的纯文本——不依赖 JSON 解析，
//! 也不解析 pnputil 的本地化文本输出（只用它的退出码）。

use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 一块 PnP 显示适配器设备。
pub struct PnpDevice {
    pub friendly_name: String,
    pub instance_id: String,
    pub status: String,
}

/// 跑一个外部程序（不弹控制台窗口），返回 (退出码, stdout+stderr 合并文本)。
pub fn run(program: &str, args: &[&str]) -> (i32, String) {
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.creation_flags(CREATE_NO_WINDOW);
    match cmd.output() {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.code().unwrap_or(-1), text)
        }
        Err(e) => (-1, format!("启动 {program} 失败：{e}")),
    }
}

/// 列出在场的显示适配器（-PresentOnly：不含已消失的幽灵设备，
/// 免得重启设备时挑中一个早就不在的实例）。失败返回错误说明（不静默）。
pub fn display_adapters() -> Result<Vec<PnpDevice>, String> {
    let script = "Get-PnpDevice -PresentOnly -Class Display | ForEach-Object { $_.FriendlyName + '|' + $_.InstanceId + '|' + $_.Status }";
    let (exit, output) = run(
        "powershell.exe",
        &["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script],
    );
    if exit != 0 {
        return Err(format!(
            "Get-PnpDevice 失败（退出码 {exit}）：{}",
            output.trim()
        ));
    }
    let mut devices = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() == 3 {
            devices.push(PnpDevice {
                friendly_name: parts[0].trim().to_string(),
                instance_id: parts[1].trim().to_string(),
                status: parts[2].trim().to_string(),
            });
        }
    }
    Ok(devices)
}

/// 枚举所有「有问题」的 PnP 设备（任意类别，状态不是 OK 的）：
/// 驱动装了但加载失败的设备（签名被拒、内存完整性拦截等）会在这里现形，带状态码。
pub fn problem_devices() -> Result<Vec<PnpDevice>, String> {
    let script = "Get-PnpDevice -PresentOnly | Where-Object { $_.Status -ne 'OK' } | ForEach-Object { $_.FriendlyName + '|' + $_.InstanceId + '|' + $_.Status }";
    let (exit, output) = run(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    );
    if exit != 0 {
        return Err(format!(
            "Get-PnpDevice（问题设备）失败（退出码 {exit}）：{}",
            output.trim()
        ));
    }
    let mut devices = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() == 3 {
            devices.push(PnpDevice {
                friendly_name: parts[0].trim().to_string(),
                instance_id: parts[1].trim().to_string(),
                status: parts[2].trim().to_string(),
            });
        }
    }
    Ok(devices)
}

/// VDD 的适配器在设备管理器里一般叫 Virtual Display Driver；不同版本命名可能不同，
/// 名字与实例 ID 都查：现行 VDD 的实例 ID 带 MTTVDD（Root\MttVDD\…），
/// IddSample 系带 IDDSAMPLE，大小写不敏感。按实例 ID 认最稳——不依赖本地化的名字。
pub fn find_vdd(devices: &[PnpDevice]) -> Option<&PnpDevice> {
    devices.iter().find(|d| {
        let name = d.friendly_name.to_lowercase();
        let id = d.instance_id.to_lowercase();
        name.contains("virtual display")
            || name.contains("virtualdisplaydriver")
            || name.contains("mttvdd")
            || name.contains("iddsample")
            || id.contains("mttvdd")
            || id.contains("iddsample")
            || id.contains("idd_sample")
    })
}

/// 禁用设备：移除手机屏时用。MttVDD 只要设备还启用着，就会按配置一直挂着一块屏，
/// 光改配置（删分辨率、恢复 count）删不掉它；禁用后它才真的从桌面上消失。
pub fn disable_device(instance_id: &str) -> (i32, String) {
    run("pnputil.exe", &["/disable-device", instance_id])
}

/// 启用设备：创建手机屏时先把移除时禁用的设备启用回来。本就启用时重复启用无害。
pub fn enable_device(instance_id: &str) -> (i32, String) {
    run("pnputil.exe", &["/enable-device", instance_id])
}

/// 重启设备让 VDD 重新读配置。有的版本上 restart 不被支持，退而求其次 disable 再 enable。
/// 返回最后一步的 (退出码, 输出)。
pub fn restart_device(instance_id: &str) -> (i32, String) {
    let (exit, output) = run("pnputil.exe", &["/restart-device", instance_id]);
    if exit == 0 {
        return (exit, output);
    }
    let (disable_exit, disable_output) = run("pnputil.exe", &["/disable-device", instance_id]);
    if disable_exit != 0 {
        return (disable_exit, disable_output);
    }
    std::thread::sleep(std::time::Duration::from_millis(1200));
    run("pnputil.exe", &["/enable-device", instance_id])
}
