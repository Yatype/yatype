//! Раскладки клавиатуры: какой символ даёт каждая клавиша.
//!
//! Таблицы строим один раз через `UCKeyTranslate` для каждой включённой
//! раскладки, поэтому годится любая: «Русская», «Русская — ПК», ABC, U.S.

use std::collections::HashMap;

use detector::Lang;

use super::ffi::*;
use engine::Key;

/// Коды клавиш, которые могут давать буквы и знаки (основной блок клавиатуры).
const MAX_CODE: u16 = 50;

pub struct KeyLayout {
    pub id: String,
    /// Источник ввода macOS — чтобы переключаться на эту раскладку.
    source: TISInputSourceRef,
    /// (код клавиши, Shift, Caps Lock) → символ.
    table: HashMap<(u16, bool, bool), char>,
}

impl KeyLayout {
    unsafe fn from_source(source: TISInputSourceRef) -> Option<KeyLayout> {
        let id = unsafe { cf_string(TISGetInputSourceProperty(source, kTISPropertyInputSourceID).cast()) }?;
        let data = unsafe { TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData) };
        if data.is_null() {
            return None;
        }
        let layout = unsafe { CFDataGetBytePtr(data) }.cast();
        let kbd_type = unsafe { LMGetKbdType() } as u32;

        let mut table = HashMap::new();
        for code in 0..=MAX_CODE {
            for shift in [false, true] {
                for caps in [false, true] {
                    // Модификаторы в формате UCKeyTranslate: (флаги Carbon >> 8).
                    let mods = (if shift { 0x02 } else { 0 }) | (if caps { 0x04 } else { 0 });
                    let mut dead = 0u32;
                    let mut len = 0usize;
                    let mut buf = [0u16; 4];
                    let status = unsafe {
                        UCKeyTranslate(
                            layout,
                            code,
                            kUCKeyActionDown,
                            mods,
                            kbd_type,
                            kUCKeyTranslateNoDeadKeysMask,
                            &mut dead,
                            buf.len(),
                            &mut len,
                            buf.as_mut_ptr(),
                        )
                    };
                    if status != 0 || len != 1 {
                        continue;
                    }
                    if let Some(c) = char::from_u32(buf[0] as u32).filter(|c| !c.is_control()) {
                        table.insert((code, shift, caps), c);
                    }
                }
            }
        }
        // Список источников освобождается, а раскладка нужна нам всё время.
        let source = unsafe { CFRetain(source.cast()) } as TISInputSourceRef;
        Some(KeyLayout { id, source, table })
    }

    /// Сделать эту раскладку текущей.
    pub fn select(&self) -> bool {
        unsafe { TISSelectInputSource(self.source) == 0 }
    }

    /// Какую клавишу нажать, чтобы получить каждый символ (для сквозного
    /// теста). Без Shift, если можно; среди одинаковых — с меньшим кодом.
    pub fn keys_for_chars(&self) -> HashMap<char, Key> {
        let mut keys: Vec<(char, Key)> = self
            .table
            .iter()
            .filter(|(key, _)| !key.2)
            .map(|(&(code, shift, _), &c)| (c, Key { code, shift, caps: false }))
            .collect();
        keys.sort_by_key(|(_, k)| std::cmp::Reverse((k.shift, k.code)));
        keys.into_iter().collect()
    }

    pub fn char_for(&self, key: &Key) -> Option<char> {
        self.table.get(&(key.code, key.shift, key.caps)).copied()
    }

    /// Текст, который дали бы эти нажатия в этой раскладке.
    pub fn read(&self, keys: &[Key]) -> String {
        keys.iter().filter_map(|k| self.char_for(k)).collect()
    }
}

unsafe fn source_lang(source: TISInputSourceRef) -> Option<Lang> {
    let langs = unsafe { TISGetInputSourceProperty(source, kTISPropertyInputSourceLanguages) };
    if langs.is_null() || unsafe { CFArrayGetCount(langs) } == 0 {
        return None;
    }
    let first = unsafe { cf_string(CFArrayGetValueAtIndex(langs, 0)) }?;
    match first.as_str() {
        "en" => Some(Lang::En),
        "ru" => Some(Lang::Ru),
        _ => None,
    }
}

/// Первая включённая раскладка для каждого языка.
pub fn enabled_layouts() -> (Option<KeyLayout>, Option<KeyLayout>) {
    let (mut en, mut ru) = (None, None);
    unsafe {
        let list = TISCreateInputSourceList(std::ptr::null(), 0);
        if list.is_null() {
            return (None, None);
        }
        for i in 0..CFArrayGetCount(list) {
            let source = CFArrayGetValueAtIndex(list, i) as TISInputSourceRef;
            let ty = TISGetInputSourceProperty(source, kTISPropertyInputSourceType);
            if ty.is_null() || CFEqual(ty, kTISTypeKeyboardLayout) == 0 {
                continue;
            }
            let slot = match source_lang(source) {
                Some(Lang::En) => &mut en,
                Some(Lang::Ru) => &mut ru,
                None => continue,
            };
            if slot.is_none() {
                *slot = KeyLayout::from_source(source);
            }
        }
        CFRelease(list);
    }
    (en, ru)
}

/// Язык раскладки, которая сейчас выбрана. `None` — раскладка не русская
/// и не английская.
pub fn current_lang() -> Option<Lang> {
    unsafe {
        let source = TISCopyCurrentKeyboardLayoutInputSource();
        if source.is_null() {
            return None;
        }
        let lang = source_lang(source);
        CFRelease(source);
        lang
    }
}
