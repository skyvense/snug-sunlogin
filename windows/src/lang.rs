//! 界面语言（简体中文 / English）：主窗口右上角切换，存 settings.txt。
//! 所有用户可见文案（界面、向导、日志）都经 tr() 取词：调用点同时写中英
//! 两份模板，插值占位符保持一致。代码注释仍用中文，不在此列。

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn is_en() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn set_en(en: bool) {
    ENGLISH.store(en, Ordering::Relaxed);
}

/// 按当前语言取文案：中文模板在前、英文在后。
pub fn tr(zh: &'static str, en: &'static str) -> &'static str {
    if is_en() {
        en
    } else {
        zh
    }
}
