//! Virtual Display Driver 的配置文件（C:\VirtualDisplayDriver\vdd_settings.xml）读写。
//! 原则：只动两处——monitors/count 和 resolutions 里手机屏那一条；其余内容逐字节保留
//! （所以用字符串手术而不是 XML 解析重序列化）；第一次动手前留一份原始备份
//! （.snug-pristine.bak，永不覆盖）；动过什么记在状态文件里，以便原样恢复。

use crate::display::{PHONE_H, PHONE_HZ, PHONE_W};
use std::path::Path;

pub const SETTINGS_PATH: &str = r"C:\VirtualDisplayDriver\vdd_settings.xml";
const SETTINGS_DIR: &str = r"C:\VirtualDisplayDriver";

/// 本应用对 VDD 配置动过什么手。
#[derive(Default)]
pub struct VddState {
    pub original_count: Option<i64>,
    pub added_resolution: bool,
    /// 配置文件是本应用新建的（原本不存在）——移除时整文件删除即回滚。
    pub created_file: bool,
}

// MARK: 状态文件（%LOCALAPPDATA%\SnugWindows\vdd-state.txt，简单的 k=v 行）

fn state_path() -> std::path::PathBuf {
    crate::log::dir().join("vdd-state.txt")
}

pub fn load_state() -> VddState {
    let mut state = VddState::default();
    if let Ok(text) = std::fs::read_to_string(state_path()) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "original_count" => state.original_count = v.trim().parse().ok(),
                    "added_resolution" => state.added_resolution = v.trim() == "1",
                    "created_file" => state.created_file = v.trim() == "1",
                    _ => {}
                }
            }
        }
    }
    state
}

pub fn save_state(state: &VddState) {
    let mut text = String::new();
    if let Some(n) = state.original_count {
        text += &format!("original_count={n}\n");
    }
    text += &format!("added_resolution={}\n", if state.added_resolution { 1 } else { 0 });
    text += &format!("created_file={}\n", if state.created_file { 1 } else { 0 });
    let _ = std::fs::create_dir_all(crate::log::dir());
    let _ = std::fs::write(state_path(), text);
}

pub fn clear_state() {
    let _ = std::fs::remove_file(state_path());
}

// MARK: 配置文件本体

pub fn settings_exists() -> bool {
    Path::new(SETTINGS_PATH).exists()
}

pub fn load_settings() -> std::io::Result<String> {
    std::fs::read_to_string(SETTINGS_PATH)
}

pub fn save_settings(text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(SETTINGS_DIR)?;
    let backup = format!("{SETTINGS_PATH}.snug-pristine.bak");
    if settings_exists() && !Path::new(&backup).exists() {
        std::fs::copy(SETTINGS_PATH, backup)?;
    }
    std::fs::write(SETTINGS_PATH, text)
}

/// 新建配置用的模板：取自 VirtualDrivers 官方仓库的最小配置，
/// 只保留功能性设置；分辨率里除常用三档外带上手机屏这一条。
pub fn create_default_settings() -> String {
    DEFAULT_SETTINGS_XML.to_string()
}

// MARK: 字符串手术

/// 在 text 里找 <tag>…</tag> 的内容区间（字节偏移），from 是开始搜索的位置。
fn content_range(text: &str, tag: &str, from: usize) -> Option<(usize, usize)> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text[from..].find(&open)? + from + open.len();
    let end = text[start..].find(&close)? + start;
    Some((start, end))
}

fn extract_uint(text: &str, tag: &str) -> Option<u32> {
    let (s, e) = content_range(text, tag, 0)?;
    text[s..e].trim().parse().ok()
}

/// 把 text 里第一个 <tag> 的整数值换掉；找不到返回 None。
fn set_uint(text: &str, tag: &str, value: u32) -> Option<String> {
    let (s, e) = content_range(text, tag, 0)?;
    let mut out = String::with_capacity(text.len() + 8);
    out.push_str(&text[..s]);
    out.push_str(&value.to_string());
    out.push_str(&text[e..]);
    Some(out)
}

pub fn get_count(text: &str) -> Option<i64> {
    let (s, e) = content_range(text, "monitors", 0)?;
    extract_uint(&text[s..e], "count").map(|n| n as i64)
}

pub fn set_count(text: &str, count: i64) -> String {
    if let Some((s, e)) = content_range(text, "monitors", 0) {
        let inner = &text[s..e];
        if let Some(new_inner) = set_uint(inner, "count", count as u32) {
            let mut out = String::with_capacity(text.len() + 16);
            out.push_str(&text[..s]);
            out.push_str(&new_inner);
            out.push_str(&text[e..]);
            return out;
        }
        // monitors 里没有 count：在 <monitors> 后面插入
        let mut out = String::with_capacity(text.len() + 24);
        out.push_str(&text[..s]);
        out.push_str(&format!("<count>{count}</count>"));
        out.push_str(&text[s..]);
        return out;
    }
    // 没有 monitors 节点：在根元素开标签后插入一整个
    if let Some(gt) = text.find('>') {
        let mut out = String::with_capacity(text.len() + 40);
        out.push_str(&text[..=gt]);
        out.push_str(&format!("<monitors><count>{count}</count></monitors>"));
        out.push_str(&text[gt + 1..]);
        return out;
    }
    text.to_string()
}

/// 所有 <resolution>…</resolution> 块的字节区间（含标签本身）。
fn resolution_blocks(text: &str) -> Vec<(usize, usize)> {
    let mut blocks = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find("<resolution>") {
        let start = from + rel;
        match text[start..].find("</resolution>") {
            Some(rel_end) => {
                let end = start + rel_end + "</resolution>".len();
                blocks.push((start, end));
                from = end;
            }
            None => break,
        }
    }
    blocks
}

fn is_phone_block(block: &str) -> bool {
    extract_uint(block, "width") == Some(PHONE_W) && extract_uint(block, "height") == Some(PHONE_H)
}

pub fn has_phone_resolution(text: &str) -> bool {
    resolution_blocks(text)
        .iter()
        .any(|&(s, e)| is_phone_block(&text[s..e]))
}

/// 配置里现有的分辨率一览（如 "1920×1080@144、1080×2346@60"），给日志取证用：
/// 解析口径和增删手术完全一致——日志里说没有，手术就一定也找不到。
pub fn resolution_summary(text: &str) -> String {
    let blocks = resolution_blocks(text);
    if blocks.is_empty() {
        return "（一条分辨率都没有）".to_string();
    }
    blocks
        .iter()
        .map(|&(s, e)| {
            let b = &text[s..e];
            let one = |tag: &str| {
                extract_uint(b, tag)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "?".to_string())
            };
            format!("{}×{}@{}", one("width"), one("height"), one("refresh_rate"))
        })
        .collect::<Vec<_>>()
        .join("、")
}

const PHONE_BLOCK: &str = "<resolution>\n            <width>1080</width>\n            <height>2346</height>\n            <refresh_rate>60</refresh_rate>\n        </resolution>";

/// 加上手机屏分辨率这一条（已有则不动）。返回新文本。
pub fn add_phone_resolution(text: &str) -> String {
    if has_phone_resolution(text) {
        return text.to_string();
    }
    // 照着已有的条目克隆，保证字段口径跟这份文件自己的写法一致；一条都没有才用标准三件套
    let blocks = resolution_blocks(text);
    let block = if let Some(&(s, e)) = blocks.first() {
        let mut b = text[s..e].to_string();
        if let Some(nb) = set_uint(&b, "width", PHONE_W) {
            b = nb;
        }
        if let Some(nb) = set_uint(&b, "height", PHONE_H) {
            b = nb;
        }
        if let Some(nb) = set_uint(&b, "refresh_rate", PHONE_HZ) {
            b = nb;
        } else if let Some(pos) = b.rfind("</resolution>") {
            b.insert_str(pos, "<refresh_rate>60</refresh_rate>");
        }
        b
    } else {
        PHONE_BLOCK.to_string()
    };
    if let Some(pos) = text.find("</resolutions>") {
        let mut out = String::with_capacity(text.len() + block.len() + 16);
        out.push_str(&text[..pos]);
        out.push_str(&block);
        out.push_str("\n        ");
        out.push_str(&text[pos..]);
        out
    } else if let Some(pos) = text.find("</vdd_settings>") {
        let mut out = String::with_capacity(text.len() + block.len() + 40);
        out.push_str(&text[..pos]);
        out.push_str("<resolutions>\n        ");
        out.push_str(&block);
        out.push_str("\n    </resolutions>\n    ");
        out.push_str(&text[pos..]);
        out
    } else {
        text.to_string()
    }
}

/// 删掉手机屏分辨率的条目。返回 (新文本, 删了几条)。
pub fn remove_phone_resolution(text: &str) -> (String, usize) {
    let blocks = resolution_blocks(text);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut removed = 0;
    for &(s, e) in &blocks {
        if is_phone_block(&text[s..e]) {
            out.push_str(&text[last..s]);
            // 连带吃掉块后面的换行+缩进，避免留下空行
            let mut end = e;
            let rest = &text[end..];
            let trimmed = rest.trim_start_matches([' ', '\t']);
            if let Some(stripped) = trimmed.strip_prefix('\n') {
                end = text.len() - stripped.len();
            } else if let Some(stripped) = trimmed.strip_prefix("\r\n") {
                end = text.len() - stripped.len();
            }
            last = end;
            removed += 1;
        }
    }
    out.push_str(&text[last..]);
    (out, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "<vdd_settings>\n    <monitors>\n        <count>2</count>\n    </monitors>\n    <resolutions>\n        <resolution>\n            <width>1920</width>\n            <height>1080</height>\n            <refresh_rate>144</refresh_rate>\n        </resolution>\n    </resolutions>\n</vdd_settings>\n";

    #[test]
    fn default_template_has_phone_and_count_1() {
        let doc = create_default_settings();
        assert_eq!(get_count(&doc), Some(1));
        assert!(has_phone_resolution(&doc));
    }

    #[test]
    fn add_phone_keeps_existing_and_count() {
        let doc = add_phone_resolution(SAMPLE);
        assert!(has_phone_resolution(&doc));
        assert_eq!(get_count(&doc), Some(2));
        // 原有的 1920×1080 条目还在，且没被改动
        assert!(doc.contains("<width>1920</width>"));
        assert!(doc.contains("<refresh_rate>144</refresh_rate>"));
        // 重复加是幂等的
        let doc2 = add_phone_resolution(&doc);
        assert_eq!(doc2.matches("<width>1080</width>").count(), 1);
    }

    #[test]
    fn remove_phone_restores_sample() {
        let doc = add_phone_resolution(SAMPLE);
        let (back, removed) = remove_phone_resolution(&doc);
        assert_eq!(removed, 1);
        assert!(!has_phone_resolution(&back));
        assert!(back.contains("<width>1920</width>"));
        assert_eq!(get_count(&back), Some(2));
    }

    #[test]
    fn resolution_summary_lists_blocks() {
        assert_eq!(resolution_summary(SAMPLE), "1920×1080@144");
        let doc = add_phone_resolution(SAMPLE);
        assert_eq!(resolution_summary(&doc), "1920×1080@144、1080×2346@60");
        assert_eq!(resolution_summary("<vdd_settings/>"), "（一条分辨率都没有）");
    }

    #[test]
    fn set_count_replaces_and_inserts() {
        assert_eq!(get_count(&set_count(SAMPLE, 5)), Some(5));
        let no_count = "<vdd_settings><monitors></monitors></vdd_settings>";
        assert_eq!(get_count(&set_count(no_count, 3)), Some(3));
        let no_monitors = "<vdd_settings><gpu/></vdd_settings>";
        assert_eq!(get_count(&set_count(no_monitors, 1)), Some(1));
    }

    #[test]
    fn add_phone_without_resolutions_node() {
        let bare = "<vdd_settings><monitors><count>1</count></monitors></vdd_settings>";
        let doc = add_phone_resolution(bare);
        assert!(has_phone_resolution(&doc));
    }
}

const DEFAULT_SETTINGS_XML: &str = r#"<?xml version='1.0' encoding='utf-8'?>
<vdd_settings>
    <monitors>
        <count>1</count>
    </monitors>
    <gpu>
        <friendlyname>default</friendlyname>
    </gpu>
    <global>
        <g_refresh_rate>60</g_refresh_rate>
        <g_refresh_rate>90</g_refresh_rate>
        <g_refresh_rate>120</g_refresh_rate>
        <g_refresh_rate>144</g_refresh_rate>
    </global>
    <resolutions>
        <resolution>
            <width>1920</width>
            <height>1080</height>
            <refresh_rate>60</refresh_rate>
        </resolution>
        <resolution>
            <width>2560</width>
            <height>1440</height>
            <refresh_rate>60</refresh_rate>
        </resolution>
        <resolution>
            <width>3840</width>
            <height>2160</height>
            <refresh_rate>60</refresh_rate>
        </resolution>
        <resolution>
            <width>1080</width>
            <height>2346</height>
            <refresh_rate>60</refresh_rate>
        </resolution>
    </resolutions>
    <logging>
        <SendLogsThroughPipe>true</SendLogsThroughPipe>
        <logging>false</logging>
        <debuglogging>false</debuglogging>
    </logging>
    <colour>
        <SDR10bit>false</SDR10bit>
        <HDRPlus>false</HDRPlus>
        <ColourFormat>RGB</ColourFormat>
    </colour>
    <cursor>
        <HardwareCursor>true</HardwareCursor>
        <CursorMaxX>128</CursorMaxX>
        <CursorMaxY>128</CursorMaxY>
        <AlphaCursorSupport>true</AlphaCursorSupport>
        <XorCursorSupportLevel>2</XorCursorSupportLevel>
    </cursor>
    <edid>
        <CustomEdid>false</CustomEdid>
        <PreventSpoof>false</PreventSpoof>
        <EdidCeaOverride>false</EdidCeaOverride>
    </edid>
</vdd_settings>
"#;
