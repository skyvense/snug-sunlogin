//! 找虚拟显示驱动的 PnP 适配器设备、重启它让新配置生效。
//! 枚举走 PowerShell 的 Get-PnpDevice 并拼成竖线分隔的纯文本——不依赖 JSON 解析，
//! 也不解析 pnputil 的本地化文本输出（只用它的退出码）。

use crate::lang::tr;
use crate::tr_fmt;
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 一块 PnP 显示适配器设备。
pub struct PnpDevice {
    pub friendly_name: String,
    pub instance_id: String,
    pub status: String,
    pub hardware_ids: Vec<String>,
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
        Err(e) => (-1, tr_fmt!("启动 {program} 失败：{e}", "Failed to start {program}: {e}")),
    }
}

/// 列出在场的显示适配器（-PresentOnly：不含已消失的幽灵设备，
/// 免得重启设备时挑中一个早就不在的实例）。失败返回错误说明（不静默）。
pub fn display_adapters() -> Result<Vec<PnpDevice>, String> {
    // 第四段是硬件 ID 列表（分号分隔）：find_vdd 靠它精确认 MttVDD 本尊。
    // 只对显示类这几台设备取属性，开销可控（全系统取属性才会慢）。
    let script = "Get-PnpDevice -PresentOnly -Class Display | ForEach-Object { $hw = (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction SilentlyContinue).Data -join ';'; $_.FriendlyName + '|' + $_.InstanceId + '|' + $_.Status + '|' + $hw }";
    let (exit, output) = run(
        "powershell.exe",
        &["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script],
    );
    if exit != 0 {
        return Err(tr_fmt!("Get-PnpDevice 失败（退出码 {exit}）：{}", "Get-PnpDevice failed (exit code {exit}): {}",
            output.trim()));
    }
    let mut devices = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() == 3 || parts.len() == 4 {
            devices.push(PnpDevice {
                friendly_name: parts[0].trim().to_string(),
                instance_id: parts[1].trim().to_string(),
                status: parts[2].trim().to_string(),
                hardware_ids: if parts.len() == 4 {
                    parts[3]
                        .split(';')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                } else {
                    Vec::new()
                },
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
        return Err(tr_fmt!("Get-PnpDevice（问题设备）失败（退出码 {exit}）：{}", "Get-PnpDevice (problem devices) failed (exit code {exit}): {}",
            output.trim()));
    }
    let mut devices = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() == 3 {
            devices.push(PnpDevice {
                friendly_name: parts[0].trim().to_string(),
                instance_id: parts[1].trim().to_string(),
                status: parts[2].trim().to_string(),
                hardware_ids: Vec::new(),
            });
        }
    }
    Ok(devices)
}

/// 在显示适配器里找 MttVDD 本尊（匹配规则见函数内注释）。
pub fn find_vdd(devices: &[PnpDevice]) -> Option<&PnpDevice> {
    // 只认 MttVDD 本尊：实例 ID 带 MTTVDD，或硬件 ID 精确等于 Root\MttVDD / MttVDD
    // （INF 里就这两个）。绝不按友好名模糊匹配——机器上常有别家的虚拟显示适配器
    // （GameViewer、AskLink 等），它们名字里同样带「Virtual Display」且枚举顺序
    // 可能在前，旧写法会先命中它们，去启用/重启/禁用别人的设备
    // （2026-11-20 同事机器实测踩中）。找不到就如实返回 None，让调用方明确报错。
    devices.iter().find(|d| {
        d.instance_id.to_lowercase().contains("mttvdd")
            || d.hardware_ids.iter().any(|h| {
                let h = h.trim().to_lowercase();
                h == "root\\mttvdd" || h == "mttvdd"
            })
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
    cycle_device(instance_id)
}

/// 禁用→停顿→启用，强制设备完整走一遍拆除/重建。
/// pnputil /restart-device 对 UMDF 设备可能只是表面成功（退出码 0 但驱动没重走
/// AddDevice，显示带不回来——2026-11-21 真机实证），这条路是显式的完整循环。
/// 返回最后一步（启用）的 (退出码, 输出)。
pub fn cycle_device(instance_id: &str) -> (i32, String) {
    let (disable_exit, disable_output) = run("pnputil.exe", &["/disable-device", instance_id]);
    if disable_exit != 0 {
        return (disable_exit, disable_output);
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    run("pnputil.exe", &["/enable-device", instance_id])
}

/// 随包 nefcon 与 VDD INF 的位置：发布包里 nefcon\nefconc.exe、vdd\MttVDD.inf 与 exe
/// 同级；INF 也可能在 C:\VirtualDisplayDriver（搞通脚本把驱动三件套放在那）。
/// 找不到返回 None（调用方如实记日志跳过，不静默）。
fn nefcon_paths() -> Option<(String, String)> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let nefcon = exe_dir.join("nefcon").join("nefconc.exe");
    let nefcon = if nefcon.exists() {
        nefcon
    } else {
        exe_dir.join("nefconc.exe")
    };
    if !nefcon.exists() {
        return None;
    }
    let inf_bundled = exe_dir.join("vdd").join("MttVDD.inf");
    let inf = if inf_bundled.exists() {
        inf_bundled
    } else {
        std::path::PathBuf::from(r"C:\VirtualDisplayDriver\MttVDD.inf")
    };
    if !inf.exists() {
        return None;
    }
    Some((
        nefcon.to_string_lossy().into_owned(),
        inf.to_string_lossy().into_owned(),
    ))
}

/// 用 nefcon 按硬件 ID 重装 VDD 节点。参考实现（snug-for-windows）的会话建屏就是
/// 这条路——它的 StartDisplay 跑 `nefcon install <inf> Root\MttVDD --no-duplicates`
/// 现建节点，而不是重启既有设备；这台机器上 VDD 安装最终也是 nefcon install 收的尾。
/// 退出码 0 = 成功，3010 = 成功但建议重启；成败最终以调用方轮询枚举结果为准。
/// 随包文件不齐返回 None。
pub fn nefcon_install() -> Option<(i32, String)> {
    let (nefcon, inf) = nefcon_paths()?;
    Some(run(
        &nefcon,
        &["install", &inf, r"Root\MttVDD", "--no-duplicates"],
    ))
}
