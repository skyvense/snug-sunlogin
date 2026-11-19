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

/// 列出所有显示适配器。失败返回错误说明（不静默）。
pub fn display_adapters() -> Result<Vec<PnpDevice>, String> {
    let script = "Get-PnpDevice -Class Display | ForEach-Object { $_.FriendlyName + '|' + $_.InstanceId + '|' + $_.Status }";
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

/// VDD 的适配器在设备管理器里叫 Virtual Display Driver（子串匹配，兼容版本差异）。
pub fn find_vdd(devices: &[PnpDevice]) -> Option<&PnpDevice> {
    devices
        .iter()
        .find(|d| d.friendly_name.contains("Virtual Display"))
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
