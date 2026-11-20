//! 手机型号与当前手机屏规格。机型库与 Mac 版共用同一份数据
//! （仓库根 Assets/phone-models.json，编译时嵌入——只有一份数据源，两边不会岔）。
//! Windows 的虚拟屏分辨率按型号比例缩放：短边以 1080 为上限（像素量对串流敏感，
//! Mac 版卡顿排查的前车之鉴），原生短边不超过 1080 的用原生值。

use std::sync::{Mutex, OnceLock};

/// 一台手机（竖握像素，短边在前）。
#[derive(Clone, PartialEq)]
pub struct PhoneModel {
    /// 品牌 + 型号，如 "Apple iPhone 17 Pro Max"（与 Mac 版存进配置的写法一致）
    pub id: String,
    pub width: u32,
    pub height: u32,
}

/// 没选过型号时的默认机型（与 Mac 版 PhoneChoice.standard 一致）。
pub const STANDARD_MODEL_ID: &str = "Apple iPhone 17 Pro Max";

const CATALOG_JSON: &str = include_str!("../../Assets/phone-models.json");

pub fn catalog() -> &'static [PhoneModel] {
    static CATALOG: OnceLock<Vec<PhoneModel>> = OnceLock::new();
    CATALOG.get_or_init(|| parse_catalog(CATALOG_JSON).unwrap_or_default())
}

fn parse_catalog(json: &str) -> Option<Vec<PhoneModel>> {
    let root: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut out = Vec::new();
    for brand in root.get("brands")?.as_array()? {
        let Some(brand_name) = brand.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(models) = brand.get("models").and_then(|v| v.as_array()) else {
            continue;
        };
        for m in models {
            let (Some(name), Some(screen)) = (
                m.get("name").and_then(|v| v.as_str()),
                m.get("screen").and_then(|v| v.as_array()),
            ) else {
                continue;
            };
            let (Some(w), Some(h)) = (
                screen.first().and_then(|v| v.as_u64()),
                screen.get(1).and_then(|v| v.as_u64()),
            ) else {
                continue;
            };
            // 只收竖握正数（机型库自己的校验在 Mac 侧测试里，这里再兜一层）
            if w == 0 || h == 0 || w > h {
                continue;
            }
            out.push(PhoneModel {
                id: format!("{brand_name} {name}"),
                width: w as u32,
                height: h as u32,
            });
        }
    }
    Some(out)
}

pub fn model_by_id(id: &str) -> Option<PhoneModel> {
    catalog().iter().find(|m| m.id == id).cloned()
}

pub fn standard_model() -> PhoneModel {
    model_by_id(STANDARD_MODEL_ID)
        .or_else(|| catalog().first().cloned())
        .unwrap_or(PhoneModel {
            id: STANDARD_MODEL_ID.to_string(),
            width: 1320,
            height: 2868,
        })
}

/// 按比例缩放：短边超过 1080 的等比缩到 1080，高取偶数（显示模式惯例用偶数尺寸）。
pub fn scaled_resolution(width: u32, height: u32) -> (u32, u32) {
    const MAX_SHORT_EDGE: u32 = 1080;
    if width <= MAX_SHORT_EDGE {
        return (width, height);
    }
    let h = (height as f64 * MAX_SHORT_EDGE as f64 / width as f64).round() as u32;
    (MAX_SHORT_EDGE, h / 2 * 2)
}

/// 当前手机屏规格（宽、高、刷新率）：启动时按存下的型号设定，换型号时更新。
/// vdd 配置手术与模式判定都读它，不再写死 1080×2346。
static SPEC: Mutex<(u32, u32, u32)> = Mutex::new((1080, 2346, 60));

pub fn current_spec() -> (u32, u32, u32) {
    *SPEC.lock().unwrap()
}

pub fn set_current_model(model: &PhoneModel) {
    let (w, h) = scaled_resolution(model.width, model.height);
    *SPEC.lock().unwrap() = (w, h, 60);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_standard_model() {
        let m = standard_model();
        assert_eq!(m.id, STANDARD_MODEL_ID);
        assert_eq!((m.width, m.height), (1320, 2868));
        assert!(catalog().len() > 20);
    }

    #[test]
    fn scaled_caps_short_edge_at_1080() {
        // 默认机型缩完正好是现行写死的 1080×2346，行为无缝衔接
        assert_eq!(scaled_resolution(1320, 2868), (1080, 2346));
        // 原生短边不超过 1080 的不放大、原样用
        assert_eq!(scaled_resolution(1080, 2400), (1080, 2400));
        assert_eq!(scaled_resolution(720, 1600), (720, 1600));
    }
}
