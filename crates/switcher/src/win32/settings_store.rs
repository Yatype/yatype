//! Настройки на Windows: простой текстовый файл
//! `%APPDATA%\Yatype\settings.txt`, который можно править в Блокноте.
//!
//! ```text
//! enabled=1
//! hotkey=0
//! combo=44,3
//! backspace_undo=0
//! excluded_app=telegram.exe
//! exception_word=ckjdj
//! ```

use std::path::PathBuf;
use std::time::SystemTime;

use engine::settings::{Combo, Hotkey, Settings};

const HEADER: &str = "\
# Настройки Yatype. Можно править в Блокноте: изменения подхватятся сами.
# excluded_app — программа, где не исправлять (имя файла, например telegram.exe).
# only_listed=1 — исправлять только в программах allowed_app=…
# exception_word — слово, которое не исправлять.
";

pub fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Yatype").join("settings.txt")
}

/// Когда файл менялся последний раз — чтобы подхватывать правки из Блокнота.
pub fn modified() -> Option<SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}

pub fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "enabled" => s.enabled = value != "0",
            "hotkey" => s.hotkey = Hotkey::from_stored(value.parse().unwrap_or(0)),
            "combo" => {
                if let Some((code, mods)) = value.split_once(',') {
                    if let (Ok(code), Ok(mods)) = (code.trim().parse(), mods.trim().parse()) {
                        s.combo = Some(Combo { code, mods });
                    }
                }
            }
            "backspace_undo" => s.backspace_undo = value == "1",
            "excluded_app" => s.excluded_apps.push(value.to_lowercase()),
            "only_listed" => s.only_listed = value == "1",
            "allowed_app" => s.allowed_apps.push(value.to_lowercase()),
            "exception_word" => s.exception_words.push(detector::model::normalize(value)),
            _ => {}
        }
    }
    s
}

pub fn format(s: &Settings) -> String {
    let mut out = String::from(HEADER);
    out += &format!("enabled={}\n", s.enabled as u8);
    out += &format!("hotkey={}\n", s.hotkey.stored());
    if let Some(c) = s.combo {
        out += &format!("combo={},{}\n", c.code, c.mods);
    }
    out += &format!("backspace_undo={}\n", s.backspace_undo as u8);
    for app in &s.excluded_apps {
        out += &format!("excluded_app={app}\n");
    }
    out += &format!("only_listed={}\n", s.only_listed as u8);
    for app in &s.allowed_apps {
        out += &format!("allowed_app={app}\n");
    }
    for word in &s.exception_words {
        out += &format!("exception_word={word}\n");
    }
    out
}

pub fn load() -> Settings {
    std::fs::read_to_string(path()).map(|t| parse(&t)).unwrap_or_default()
}

pub fn save(s: &Settings) {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format(s));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Settings {
            enabled: false,
            hotkey: Hotkey::Combo,
            combo: Some(Combo { code: 0x2C, mods: 3 }),
            backspace_undo: true,
            excluded_apps: vec!["telegram.exe".into()],
            only_listed: true,
            allowed_apps: vec!["winword.exe".into()],
            exception_words: vec!["ckjdj".into()],
        };
        let back = parse(&format(&s));
        assert_eq!(back.enabled, s.enabled);
        assert_eq!(back.hotkey, s.hotkey);
        assert_eq!(back.combo, s.combo);
        assert_eq!(back.backspace_undo, s.backspace_undo);
        assert_eq!(back.excluded_apps, s.excluded_apps);
        assert_eq!(back.only_listed, s.only_listed);
        assert_eq!(back.allowed_apps, s.allowed_apps);
        assert_eq!(back.exception_words, s.exception_words);
    }

    #[test]
    fn empty_file_gives_defaults() {
        assert!(parse("").enabled);
    }
}
