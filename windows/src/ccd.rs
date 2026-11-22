//! CCD（Connecting and Configuring Displays）拓扑层：查/改显示路径的激活状态。
//!
//! 为什么需要这一层（2026-11-21 真机定案）：VDD 设备 Started、监视器 Started、
//! 驱动管道在、配置正确，虚拟屏仍可能不出现在桌面枚举里——它的显示路径在
//! 持久化拓扑里是未激活的（设置→显示里呈「仅在 1 上显示」、2 号屏灰着）。
//! 向日葵每次会话结束都会把虚拟屏路径摘掉并存盘，重启机器也不恢复；
//! 设备层动作（pnputil 重启、nefcon 重装、禁用启用循环）都改不了这一层，
//! 只有 SetDisplayConfig 把路径重新激活才行（等价于在设置里手动选
//! 「扩展这些显示器」并保留更改，真机已验）。

use crate::lang::tr;
use crate::tr_fmt;
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
    let (paths, _modes) = query_all().map_err(|e| tr_fmt!("查询显示拓扑失败（错误码 {e}）", "Failed to query the display topology (error code {e})"))?;
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
            out.notes.push(tr_fmt!("VDD 路径：目标「{friendly}」，激活={}", "VDD path: target \"{friendly}\", active={}",
                if out.was_active { tr("是", "yes") } else { tr("否", "no") }));
        }
    }
    if !out.found {
        out.notes.push(tr("拓扑里没有 VDD 的显示路径", "No VDD display path in the topology").to_string());
    }
    Ok(out)
}

/// 把 VDD 路径激活并存盘。已经激活则什么都不做（applied=false）。
/// 模式索引失效时按当前手机屏规格补源/目标模式，源位置接在现有桌面最右侧。
pub fn activate_vdd() -> Result<Outcome, String> {
    let (mut paths, mut modes) =
        query_all().map_err(|e| tr_fmt!("查询显示拓扑失败（错误码 {e}）", "Failed to query the display topology (error code {e})"))?;
    let mut out = Outcome {
        found: false,
        was_active: false,
        applied: false,
        notes: Vec::new(),
    };
    let Some(idx) = paths.iter().position(|p| {
        let (friendly, devpath) = target_name(p);
        if is_vdd_target(&friendly, &devpath) {
            out.notes.push(tr_fmt!("VDD 路径：目标「{friendly}」，激活={}，源模式索引 {}，目标模式索引 {}", "VDD path: target \"{friendly}\", active={}, source mode index {}, target mode index {}",
                if p.flags & PATH_ACTIVE != 0 { tr("是", "yes") } else { tr("否", "no") },
                unsafe { p.sourceInfo.Anonymous.modeInfoIdx },
                unsafe { p.targetInfo.Anonymous.modeInfoIdx },));
            true
        } else {
            false
        }
    }) else {
        out.notes.push(tr("拓扑里没有 VDD 的显示路径", "No VDD display path in the topology").to_string());
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
                .push(tr_fmt!("源模式索引失效，已补 {pw}×{ph} 源模式（位置 {right},0）", "Source mode index was invalid; added a {pw}×{ph} source mode (position {right},0)"));
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
                .push(tr_fmt!("目标模式索引失效，已补 {pw}×{ph}@{phz} 目标模式", "Target mode index was invalid; added a {pw}×{ph}@{phz} target mode"));
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
        .push(tr_fmt!("SetDisplayConfig 返回 {rc}（0 = 成功）", "SetDisplayConfig returned {rc} (0 = success)"));
    out.applied = rc == 0;
    Ok(out)
}

// MARK: 高级模式「仅输出虚拟屏」：关/恢复物理屏

/// 关/恢复物理屏的结果：过程说明逐条带回记日志；applied 只认复查后的真实拓扑。
pub struct PhysicalOutcome {
    /// SetDisplayConfig 成功且复查与预期一致
    pub applied: bool,
    /// 本次真正被关掉/恢复的物理屏条数（0 = 本来就已是目标状态）
    pub changed: usize,
    pub notes: Vec<String>,
}

/// 是不是物理屏的路径：目标在场、不是 VDD 本屏、也不是别家的虚拟显示
/// （向日葵自己的 ORAY、通用 IDD 等一律不碰——宁可少关一块，不误关别家的）。
fn is_physical_target(friendly: &str, device_path: &str) -> bool {
    if is_vdd_target(friendly, device_path) {
        return false;
    }
    let hay = format!("{friendly} {device_path}").to_uppercase();
    !["VDD", "VIRTUAL", "ORAY", "IDD", "INDIRECT"]
        .iter()
        .any(|k| hay.contains(k))
}

/// 物理路径清单：(路径下标, 友好名, 是否激活)。目标不在场的路径不算数。
fn physical_paths(
    paths: &[DISPLAYCONFIG_PATH_INFO],
) -> Vec<(usize, String, bool)> {
    let mut out = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        if !p.targetInfo.targetAvailable.as_bool() {
            continue;
        }
        let (friendly, devpath) = target_name(p);
        if is_physical_target(&friendly, &devpath) {
            out.push((i, friendly, p.flags & PATH_ACTIVE != 0));
        }
    }
    out
}

fn luid_eq(a: windows::Win32::Foundation::LUID, b: windows::Win32::Foundation::LUID) -> bool {
    a.LowPart == b.LowPart && a.HighPart == b.HighPart
}

/// 在模式数组里找同 id + 同适配器的模式下标（拓扑库里常有同目标的历史条目可借）。
fn find_mode(
    modes: &[DISPLAYCONFIG_MODE_INFO],
    info_type: DISPLAYCONFIG_MODE_INFO_TYPE,
    id: u32,
    adapter: windows::Win32::Foundation::LUID,
) -> Option<u32> {
    modes.iter().position(|m| {
        m.infoType == info_type && m.id == id && luid_eq(m.adapterId, adapter)
    }).map(|i| i as u32)
}

/// 关掉所有物理屏，只留 VDD 虚拟屏输出。
/// 安全前提：VDD 路径必须已激活，否则一块屏都不关（绝不把用户留在无屏状态）。
/// 应用时故意不存盘（不带 SDC_SAVE_TO_DATABASE）：持久化拓扑里物理屏仍是开的，
/// 万一程序异常退出，下次开机物理屏自动回来——这是最后一道保险。
pub fn disable_physicals() -> Result<PhysicalOutcome, String> {
    let (mut paths, mut modes) =
        query_all().map_err(|e| tr_fmt!("查询显示拓扑失败（错误码 {e}）", "Failed to query the display topology (error code {e})"))?;
    let mut out = PhysicalOutcome {
        applied: false,
        changed: 0,
        notes: Vec::new(),
    };
    // 先确认 VDD 在且激活
    let vdd_active = paths.iter().any(|p| {
        let (friendly, devpath) = target_name(p);
        is_vdd_target(&friendly, &devpath) && p.flags & PATH_ACTIVE != 0
    });
    if !vdd_active {
        return Err(tr("VDD 虚拟屏路径不在或未激活，已放弃关物理屏（避免无屏可用）", "The VDD virtual screen path is missing or inactive; giving up on turning off the physical displays (to avoid ending up with no usable screen)").to_string());
    }
    let phys = physical_paths(&paths);
    // 历史路径大多本来就关着，只记开着的（要关的就是它们），其余给个总数即可
    for (idx, name, active) in &phys {
        if *active {
            out.notes
                .push(tr_fmt!("关掉物理屏：目标「{name}」", "Turning off physical displays: target \"{name}\""));
            paths[*idx].flags &= !PATH_ACTIVE;
            out.changed += 1;
        }
    }
    out.notes.push(tr_fmt!("物理屏路径共 {} 条，本次关掉 {} 条", "Physical display paths: {} in total, turning off {} this time",
        phys.len(),
        out.changed));
    if out.changed == 0 {
        out.notes.push(tr("没有处于开启状态的物理屏，无需处理", "No physical display is on; nothing to do").to_string());
        return Ok(out);
    }
    // 关掉物理屏后，剩下的虚拟屏必须顶到原点 (0,0)：SetDisplayConfig 要求至少
    // 一条激活路径的源在原点（主屏位置），否则返回 87 参数错误（2026-11-22 实测：
    // 物理屏在 (0,0)、虚拟屏在 (1920,0) 时直接关物理屏必失败）。
    match paths.iter().position(|p| {
        let (friendly, devpath) = target_name(p);
        is_vdd_target(&friendly, &devpath) && p.flags & PATH_ACTIVE != 0
    }) {
        Some(vdd_idx) => {
            let smi = unsafe { paths[vdd_idx].sourceInfo.Anonymous.modeInfoIdx };
            if smi == MODE_IDX_INVALID {
                return Err(tr("虚拟屏的源模式索引失效，放弃关物理屏", "The virtual screen's source mode index is invalid; giving up on turning off the physical displays").to_string());
            }
            if let Some(m) = modes.get_mut(smi as usize) {
                m.Anonymous.sourceMode.position = POINTL { x: 0, y: 0 };
                out.notes.push(tr("虚拟屏源位置移到 (0,0) 顶替主屏", "Virtual screen source moved to (0,0) to take over as primary").to_string());
            }
        }
        None => return Err(tr("关物理屏前复查不到激活的虚拟屏路径，放弃", "No active virtual screen path found in the final check before turning off physical displays; giving up").to_string()),
    }
    let rc = unsafe {
        SetDisplayConfig(
            Some(&paths),
            Some(&modes),
            SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES,
        )
    };
    out.notes
        .push(tr_fmt!("SetDisplayConfig 返回 {rc}（0 = 成功，未存盘）", "SetDisplayConfig returned {rc} (0 = success, not saved)"));
    if rc != 0 {
        return Ok(out);
    }
    // 复查：物理屏应当全部不激活，VDD 应当还在
    let (paths2, _m2) =
        query_all().map_err(|e| tr_fmt!("关物理屏后复查拓扑失败（错误码 {e}）", "Rechecking the topology after turning off physical displays failed (error code {e})"))?;
    let still_on = physical_paths(&paths2)
        .iter()
        .filter(|(_, _, a)| *a)
        .count();
    let vdd_still = paths2.iter().any(|p| {
        let (friendly, devpath) = target_name(p);
        is_vdd_target(&friendly, &devpath) && p.flags & PATH_ACTIVE != 0
    });
    out.notes.push(tr_fmt!("复查：仍开着的物理屏 {still_on} 块，VDD 激活={}", "Recheck: {still_on} physical display(s) still on, VDD active={}",
        if vdd_still { tr("是", "yes") } else { tr("否", "no") }));
    out.applied = still_on == 0 && vdd_still;
    Ok(out)
}

/// 把物理屏全部恢复开启（断开/退出/启动自检时调用，必须成功或明确报错）。
/// 拓扑库里同一物理目标常有多条历史路径（大多关着），按目标分组处理：
/// 每组选一条代表路径（已激活的优先，其次模式索引最全的），只激活关着的组；
/// 索引失效的按「借同源/同目标历史模式 → 按源尺寸现造 60Hz」顺序补。
/// 位置统一重排：物理屏从 (0,0) 向右，VDD 在场时排最右。应用时存盘，
/// 把持久化拓扑一并修回正常状态（向日葵摘 VDD 的存盘改动由建屏阶梯另行处理）。
/// 恢复物理屏：先走路径手术（补模式、激活、重排位置）；手术未生效时整体套用
/// 存盘拓扑兜底——关物理屏从不存盘，存盘拓扑里物理屏必然是开的。
pub fn restore_physicals() -> Result<PhysicalOutcome, String> {
    let mut out = match restore_physicals_surgery() {
        Ok(o) if o.applied || o.changed == 0 => return Ok(o),
        Ok(o) => o,
        Err(e) => PhysicalOutcome {
            applied: false,
            changed: 1,
            notes: vec![tr_fmt!("路径手术失败：{e}", "Path surgery failed: {e}")],
        },
    };
    out.notes.push(tr("改用存盘拓扑整体恢复", "Falling back to applying the saved topology as a whole").to_string());
    if let Err(fe) = apply_database_config(&mut out) {
        return Err(tr_fmt!("{fe}；路径手术同样未生效：{}", "{fe}; the path surgery also had no effect: {}",
            out.notes.join("；")));
    }
    Ok(out)
}

/// 整体套用存盘拓扑（SDC_USE_DATABASE_CURRENT）。2026-11-22 实测：强杀进程后
/// 重启，路径手术因物理屏源模式缺失且无处可借而失败，此法返回 0、物理屏恢复。
fn apply_database_config(out: &mut PhysicalOutcome) -> Result<(), String> {
    let rc = unsafe { SetDisplayConfig(None, None, SDC_APPLY | SDC_USE_DATABASE_CURRENT) };
    out.notes
        .push(tr_fmt!("套用存盘拓扑：SetDisplayConfig 返回 {rc}（0 = 成功）", "Applying the saved topology: SetDisplayConfig returned {rc} (0 = success)"));
    if rc != 0 {
        return Err(tr_fmt!("套用存盘拓扑失败（错误码 {rc}）", "Applying the saved topology failed (error code {rc})"));
    }
    let (paths2, _m) =
        query_all().map_err(|e| tr_fmt!("套用存盘拓扑后复查拓扑失败（错误码 {e}）", "Rechecking the topology after applying the saved topology failed (error code {e})"))?;
    let any_on = physical_paths(&paths2).iter().any(|(_, _, a)| *a);
    out.notes.push(tr_fmt!("复查：物理屏有激活路径={}", "Recheck: a physical display has an active path={}",
        if any_on { tr("是", "yes") } else { tr("否", "no") }));
    if !any_on {
        return Err(tr("套用存盘拓扑后物理屏仍未激活", "Physical displays still not active after applying the saved topology").to_string());
    }
    out.applied = true;
    Ok(())
}

fn restore_physicals_surgery() -> Result<PhysicalOutcome, String> {
    let (mut paths, mut modes) =
        query_all().map_err(|e| tr_fmt!("查询显示拓扑失败（错误码 {e}）", "Failed to query the display topology (error code {e})"))?;
    let mut out = PhysicalOutcome {
        applied: false,
        changed: 0,
        notes: Vec::new(),
    };
    let phys = physical_paths(&paths);
    // 按目标（adapterId + target id）分组，保留出现顺序
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (idx, _name, _a) in &phys {
        let t = &paths[*idx].targetInfo;
        if let Some(g) = groups.iter_mut().find(|g| {
            let t0 = &paths[g[0]].targetInfo;
            t0.id == t.id && luid_eq(t0.adapterId, t.adapterId)
        }) {
            g.push(*idx);
        } else {
            groups.push(vec![*idx]);
        }
    }
    // 每组选代表：已激活的优先；都没激活时选模式索引最全的
    let mut chosen: Vec<usize> = Vec::new();
    let mut off_groups = 0usize;
    for g in &groups {
        let active_one = g
            .iter()
            .copied()
            .find(|&i| paths[i].flags & PATH_ACTIVE != 0);
        let pick = match active_one {
            Some(i) => i,
            None => {
                off_groups += 1;
                g.iter()
                    .copied()
                    .max_by_key(|&i| {
                        let s_ok = unsafe { paths[i].sourceInfo.Anonymous.modeInfoIdx }
                            != MODE_IDX_INVALID;
                        let t_ok = unsafe { paths[i].targetInfo.Anonymous.modeInfoIdx }
                            != MODE_IDX_INVALID;
                        (s_ok as u8) + (t_ok as u8)
                    })
                    .unwrap_or(g[0])
            }
        };
        chosen.push(pick);
    }
    if off_groups == 0 {
        out.notes.push(tr("物理屏本来就全部开着，无需恢复", "All physical displays are already on; no restore needed").to_string());
        return Ok(out);
    }
    // 给关着的组补齐模式索引并激活
    for &i in &chosen {
        let group_off = paths[i].flags & PATH_ACTIVE == 0
            && !groups
                .iter()
                .find(|g| g.contains(&i))
                .map(|g| {
                    g.iter()
                        .any(|&j| paths[j].flags & PATH_ACTIVE != 0)
                })
                .unwrap_or(false);
        if !group_off {
            continue;
        }
        unsafe {
            if paths[i].sourceInfo.Anonymous.modeInfoIdx == MODE_IDX_INVALID {
                let sid = paths[i].sourceInfo.id;
                let sad = paths[i].sourceInfo.adapterId;
                if let Some(mi) =
                    find_mode(&modes, DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, sid, sad)
                {
                    paths[i].sourceInfo.Anonymous.modeInfoIdx = mi;
                    out.notes
                        .push(tr("物理屏源模式索引失效，已借用同源的历史模式", "A physical display's source mode index was invalid; borrowed a historical mode from the same source").to_string());
                }
            }
            if paths[i].targetInfo.Anonymous.modeInfoIdx == MODE_IDX_INVALID {
                let tid = paths[i].targetInfo.id;
                let tad = paths[i].targetInfo.adapterId;
                if let Some(mi) =
                    find_mode(&modes, DISPLAYCONFIG_MODE_INFO_TYPE_TARGET, tid, tad)
                {
                    paths[i].targetInfo.Anonymous.modeInfoIdx = mi;
                    out.notes
                        .push(tr("物理屏目标模式索引失效，已借用同目标的历史模式", "A physical display's target mode index was invalid; borrowed a historical mode for the same target").to_string());
                }
            }
            if paths[i].targetInfo.Anonymous.modeInfoIdx == MODE_IDX_INVALID {
                let smi = paths[i].sourceInfo.Anonymous.modeInfoIdx;
                if smi == MODE_IDX_INVALID {
                    return Err(tr("物理屏的源/目标模式都缺失，无法恢复（请重启或在设置→显示里手动开启）", "Both source and target modes are missing for a physical display; cannot restore (reboot, or turn it on manually under Settings → Display)").to_string());
                }
                let sm = modes[smi as usize].Anonymous.sourceMode;
                let (w, h, hz) = (sm.width, sm.height, 60u32);
                let mut signal = DISPLAYCONFIG_VIDEO_SIGNAL_INFO::default();
                signal.pixelRate = (w as u64) * (h as u64) * (hz as u64);
                signal.hSyncFreq = DISPLAYCONFIG_RATIONAL {
                    Numerator: h * hz,
                    Denominator: 1,
                };
                signal.vSyncFreq = DISPLAYCONFIG_RATIONAL {
                    Numerator: hz,
                    Denominator: 1,
                };
                signal.activeSize = DISPLAYCONFIG_2DREGION { cx: w, cy: h };
                signal.totalSize = DISPLAYCONFIG_2DREGION { cx: w, cy: h };
                signal.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
                modes.push(DISPLAYCONFIG_MODE_INFO {
                    infoType: DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
                    id: paths[i].targetInfo.id,
                    adapterId: paths[i].targetInfo.adapterId,
                    Anonymous: DISPLAYCONFIG_MODE_INFO_0 {
                        targetMode: DISPLAYCONFIG_TARGET_MODE {
                            targetVideoSignalInfo: signal,
                        },
                    },
                });
                paths[i].targetInfo.Anonymous.modeInfoIdx = (modes.len() - 1) as u32;
                paths[i].targetInfo.rotation = DISPLAYCONFIG_ROTATION_IDENTITY;
                paths[i].targetInfo.scaling = DISPLAYCONFIG_SCALING_IDENTITY;
                paths[i].targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
                    Numerator: hz,
                    Denominator: 1,
                };
                paths[i].targetInfo.scanLineOrdering =
                    DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
                out.notes
                    .push(tr_fmt!("物理屏目标模式缺失，已按源模式现造 {w}×{h}@{hz}", "A physical display's target mode was missing; created {w}×{h}@{hz} from its source mode"));
            }
            if paths[i].sourceInfo.Anonymous.modeInfoIdx == MODE_IDX_INVALID {
                return Err(tr("物理屏的源模式缺失且无处可借，无法恢复（请重启或在设置→显示里手动开启）", "A physical display's source mode is missing with nothing to borrow; cannot restore (reboot, or turn it on manually under Settings → Display)").to_string());
            }
        }
        paths[i].flags |= PATH_ACTIVE;
        out.changed += 1;
        let (friendly, _dp) = target_name(&paths[i]);
        out.notes.push(tr_fmt!("恢复物理屏：目标「{friendly}」", "Restoring physical display: target \"{friendly}\""));
    }
    // 位置统一重排：各组代表路径的源模式从 (0,0) 向右排，VDD 在场时排最右。
    // 源模式可能被同源的多条路径共享，改一次即可；重复改同一模式结果一致。
    let mut cursor = 0i32;
    for &i in &chosen {
        let smi = unsafe { paths[i].sourceInfo.Anonymous.modeInfoIdx };
        if smi != MODE_IDX_INVALID {
            if let Some(m) = modes.get_mut(smi as usize) {
                m.Anonymous.sourceMode.position = POINTL { x: cursor, y: 0 };
                cursor += unsafe { m.Anonymous.sourceMode.width } as i32;
            }
        }
    }
    if let Some(vi) = paths.iter().position(|p| {
        let (friendly, devpath) = target_name(p);
        is_vdd_target(&friendly, &devpath) && p.flags & PATH_ACTIVE != 0
    }) {
        let smi = unsafe { paths[vi].sourceInfo.Anonymous.modeInfoIdx };
        if smi != MODE_IDX_INVALID {
            if let Some(m) = modes.get_mut(smi as usize) {
                m.Anonymous.sourceMode.position = POINTL { x: cursor, y: 0 };
            }
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
        .push(tr_fmt!("SetDisplayConfig 返回 {rc}（0 = 成功）", "SetDisplayConfig returned {rc} (0 = success)"));
    if rc != 0 {
        return Ok(out);
    }
    // 复查：每个物理目标都应至少有一条激活路径（历史路径关着不算问题）
    let (paths2, _m2) =
        query_all().map_err(|e| tr_fmt!("恢复物理屏后复查拓扑失败（错误码 {e}）", "Rechecking the topology after restoring the physical displays failed (error code {e})"))?;
    let p2 = physical_paths(&paths2);
    let mut keys: Vec<(u32, u32, i32)> = Vec::new();
    for (idx, _n, _a) in &p2 {
        let t = &paths2[*idx].targetInfo;
        let key = (t.id, t.adapterId.LowPart, t.adapterId.HighPart);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    let mut targets_off = 0usize;
    for key in &keys {
        let any_on = p2.iter().any(|(idx, _n, a)| {
            let t = &paths2[*idx].targetInfo;
            (t.id, t.adapterId.LowPart, t.adapterId.HighPart) == *key && *a
        });
        if !any_on {
            targets_off += 1;
        }
    }
    out.notes.push(tr_fmt!("复查：物理目标 {} 个，仍全部关着的 {} 个", "Recheck: {} physical target(s), {} still fully off",
        keys.len(),
        targets_off));
    out.applied = targets_off == 0;
    Ok(out)
}
