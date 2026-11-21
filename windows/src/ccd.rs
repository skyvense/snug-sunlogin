//! CCD（Connecting and Configuring Displays）拓扑层：查/改显示路径的激活状态。
//!
//! 为什么需要这一层（2026-11-21 真机定案）：VDD 设备 Started、监视器 Started、
//! 驱动管道在、配置正确，虚拟屏仍可能不出现在桌面枚举里——它的显示路径在
//! 持久化拓扑里是未激活的（设置→显示里呈「仅在 1 上显示」、2 号屏灰着）。
//! 向日葵每次会话结束都会把虚拟屏路径摘掉并存盘，重启机器也不恢复；
//! 设备层动作（pnputil 重启、nefcon 重装、禁用启用循环）都改不了这一层，
//! 只有 SetDisplayConfig 把路径重新激活才行（等价于在设置里手动选
//! 「扩展这些显示器」并保留更改，真机已验）。

use windows::Win32::Devices::Display::*;
use windows::Win32::Foundation::POINTL;

// DISPLAYCONFIG_PATH_INFO.flags 的激活位（SDK 里叫 DISPLAYCONFIG_PATH_ACTIVE）
const PATH_ACTIVE: u32 = 0x0000_0001;
const MODE_IDX_INVALID: u32 = 0xFFFF_FFFF;

/// 一次拓扑检查/激活的结果：过程说明逐条带回给调用方记日志，不在这里吞掉。
pub struct Outcome {
    /// 找到 VDD 的显示路径（不管激活与否）
    pub found: bool,
    /// 找到时它已经是激活的
    pub was_active: bool,
    /// 本次调用真的调了 SetDisplayConfig 并成功（返回码 0）
    pub applied: bool,
    pub notes: Vec<String>,
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// 是不是 MttVDD 的监视器：PnP 实例是 DISPLAY\MTT1337，友好名带 VDD by MTT。
/// 绝不宽松匹配 "Virtual" 字样——别家虚拟屏（向日葵自己的 ORAYDISPLAY 等）不能动。
fn is_vdd_target(friendly: &str, device_path: &str) -> bool {
    device_path.contains("MTT1337")
        || friendly.contains("VDD by MTT")
        || friendly.contains("MttVDD")
}

fn query_all() -> Result<
    (
        Vec<DISPLAYCONFIG_PATH_INFO>,
        Vec<DISPLAYCONFIG_MODE_INFO>,
    ),
    u32,
> {
    unsafe {
        let mut npaths = 0u32;
        let mut nmodes = 0u32;
        let err = GetDisplayConfigBufferSizes(QDC_ALL_PATHS, &mut npaths, &mut nmodes);
        if err.0 != 0 {
            return Err(err.0);
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); npaths as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nmodes as usize];
        let err = QueryDisplayConfig(
            QDC_ALL_PATHS,
            &mut npaths,
            paths.as_mut_ptr(),
            &mut nmodes,
            modes.as_mut_ptr(),
            None,
        );
        if err.0 != 0 {
            return Err(err.0);
        }
        paths.truncate(npaths as usize);
        modes.truncate(nmodes as usize);
        Ok((paths, modes))
    }
}

/// 取一条路径的目标监视器（友好名, 设备路径）；取不到返回空串。
fn target_name(path: &DISPLAYCONFIG_PATH_INFO) -> (String, String) {
    let mut name = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
    name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
    name.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
    name.header.adapterId = path.targetInfo.adapterId;
    name.header.id = path.targetInfo.id;
    let rc = unsafe { DisplayConfigGetDeviceInfo(&mut name.header as *mut _) };
    if rc != 0 {
        return (String::new(), String::new());
    }
    (
        from_wide(&name.monitorFriendlyDeviceName),
        from_wide(&name.monitorDevicePath),
    )
}

/// 只查不改：VDD 路径在不在、激活与否。
pub fn vdd_path_state() -> Result<Outcome, String> {
    let (paths, _modes) = query_all().map_err(|e| format!("查询显示拓扑失败（错误码 {e}）"))?;
    let mut out = Outcome {
        found: false,
        was_active: false,
        applied: false,
        notes: Vec::new(),
    };
    for p in &paths {
        let (friendly, devpath) = target_name(p);
        if is_vdd_target(&friendly, &devpath) {
            out.found = true;
            out.was_active = p.flags & PATH_ACTIVE != 0;
            out.notes.push(format!(
                "VDD 路径：目标「{friendly}」，激活={}",
                if out.was_active { "是" } else { "否" }
            ));
        }
    }
    if !out.found {
        out.notes.push("拓扑里没有 VDD 的显示路径".to_string());
    }
    Ok(out)
}

/// 把 VDD 路径激活并存盘。已经激活则什么都不做（applied=false）。
/// 模式索引失效时按当前手机屏规格补源/目标模式，源位置接在现有桌面最右侧。
pub fn activate_vdd() -> Result<Outcome, String> {
    let (mut paths, mut modes) =
        query_all().map_err(|e| format!("查询显示拓扑失败（错误码 {e}）"))?;
    let mut out = Outcome {
        found: false,
        was_active: false,
        applied: false,
        notes: Vec::new(),
    };
    let Some(idx) = paths.iter().position(|p| {
        let (friendly, devpath) = target_name(p);
        if is_vdd_target(&friendly, &devpath) {
            out.notes.push(format!(
                "VDD 路径：目标「{friendly}」，激活={}，源模式索引 {}，目标模式索引 {}",
                if p.flags & PATH_ACTIVE != 0 { "是" } else { "否" },
                unsafe { p.sourceInfo.Anonymous.modeInfoIdx },
                unsafe { p.targetInfo.Anonymous.modeInfoIdx },
            ));
            true
        } else {
            false
        }
    }) else {
        out.notes.push("拓扑里没有 VDD 的显示路径".to_string());
        return Ok(out);
    };
    out.found = true;
    out.was_active = paths[idx].flags & PATH_ACTIVE != 0;
    if out.was_active {
        return Ok(out);
    }

    let (pw, ph, phz) = crate::phones::current_spec();
    // 现有桌面右边界：新源模式摆在它右边，不和主屏重叠
    let mut right = 0i32;
    for p in &paths {
        if p.flags & PATH_ACTIVE == 0 {
            continue;
        }
        let mi = unsafe { p.sourceInfo.Anonymous.modeInfoIdx };
        if mi != MODE_IDX_INVALID {
            if let Some(m) = modes.get(mi as usize) {
                let sm = unsafe { m.Anonymous.sourceMode };
                right = right.max(sm.position.x + sm.width as i32);
            }
        }
    }

    paths[idx].flags |= PATH_ACTIVE;
    unsafe {
        if paths[idx].sourceInfo.Anonymous.modeInfoIdx == MODE_IDX_INVALID {
            modes.push(DISPLAYCONFIG_MODE_INFO {
                infoType: DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
                id: paths[idx].sourceInfo.id,
                adapterId: paths[idx].sourceInfo.adapterId,
                Anonymous: DISPLAYCONFIG_MODE_INFO_0 {
                    sourceMode: DISPLAYCONFIG_SOURCE_MODE {
                        width: pw,
                        height: ph,
                        pixelFormat: DISPLAYCONFIG_PIXELFORMAT_32BPP,
                        position: POINTL { x: right, y: 0 },
                    },
                },
            });
            paths[idx].sourceInfo.Anonymous.modeInfoIdx = (modes.len() - 1) as u32;
            out.notes
                .push(format!("源模式索引失效，已补 {pw}×{ph} 源模式（位置 {right},0）"));
        }
        if paths[idx].targetInfo.Anonymous.modeInfoIdx == MODE_IDX_INVALID {
            let mut signal = DISPLAYCONFIG_VIDEO_SIGNAL_INFO::default();
            signal.pixelRate = (pw as u64) * (ph as u64) * (phz as u64);
            signal.hSyncFreq = DISPLAYCONFIG_RATIONAL {
                Numerator: ph * phz,
                Denominator: 1,
            };
            signal.vSyncFreq = DISPLAYCONFIG_RATIONAL {
                Numerator: phz,
                Denominator: 1,
            };
            signal.activeSize = DISPLAYCONFIG_2DREGION { cx: pw, cy: ph };
            signal.totalSize = DISPLAYCONFIG_2DREGION { cx: pw, cy: ph };
            signal.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
            modes.push(DISPLAYCONFIG_MODE_INFO {
                infoType: DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
                id: paths[idx].targetInfo.id,
                adapterId: paths[idx].targetInfo.adapterId,
                Anonymous: DISPLAYCONFIG_MODE_INFO_0 {
                    targetMode: DISPLAYCONFIG_TARGET_MODE {
                        targetVideoSignalInfo: signal,
                    },
                },
            });
            paths[idx].targetInfo.Anonymous.modeInfoIdx = (modes.len() - 1) as u32;
            paths[idx].targetInfo.rotation = DISPLAYCONFIG_ROTATION_IDENTITY;
            paths[idx].targetInfo.scaling = DISPLAYCONFIG_SCALING_IDENTITY;
            paths[idx].targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
                Numerator: phz,
                Denominator: 1,
            };
            paths[idx].targetInfo.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
            out.notes
                .push(format!("目标模式索引失效，已补 {pw}×{ph}@{phz} 目标模式"));
        }
    }

    let rc = unsafe {
        SetDisplayConfig(
            Some(&paths),
            Some(&modes),
            SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES | SDC_SAVE_TO_DATABASE,
        )
    };
    out.notes
        .push(format!("SetDisplayConfig 返回 {rc}（0 = 成功）"));
    out.applied = rc == 0;
    Ok(out)
}
