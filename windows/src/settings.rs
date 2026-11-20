//! 本应用自己的设置（%LOCALAPPDATA%\SnugWindows\settings.txt，简单的 k=v 行）：
//! 选的手机型号 + 是否主控连入自动建屏。与 vdd-state.txt 同目录、同格式。

use std::path::PathBuf;

pub struct Settings {
    pub phone_model_id: String,
    pub auto_create: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            phone_model_id: crate::phones::STANDARD_MODEL_ID.to_string(),
            auto_create: true,
        }
    }
}

fn path() -> PathBuf {
    crate::log::dir().join("settings.txt")
}

pub fn load() -> Settings {
    let mut s = Settings::default();
    if let Ok(text) = std::fs::read_to_string(path()) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "phone_model" => s.phone_model_id = v.trim().to_string(),
                    "auto_create" => s.auto_create = v.trim() != "0",
                    _ => {}
                }
            }
        }
    }
    s
}

pub fn save(s: &Settings) {
    let text = format!(
        "phone_model={}\nauto_create={}\n",
        s.phone_model_id,
        if s.auto_create { 1 } else { 0 }
    );
    let _ = std::fs::create_dir_all(crate::log::dir());
    let _ = std::fs::write(path(), text);
}
