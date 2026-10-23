//! Названия клавиш Windows для горячих клавиш. Код клавиши — скан-код,
//! у расширенных клавиш (стрелки, Home…) добавлен бит 0x100.

use engine::settings::{Combo, mods};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyNameTextW;

use super::from_wide;

pub const ESCAPE: u16 = 0x01;

/// F1–F24: их можно назначить и без модификаторов.
pub fn is_function_key(code: u16) -> bool {
    matches!(code, 0x3B..=0x44 | 0x57 | 0x58 | 0x64..=0x6E | 0x76)
}

fn key_name(code: u16) -> String {
    let scan = (code & 0xFF) as i32;
    let extended = if code & 0x100 != 0 { 1 << 24 } else { 0 };
    let mut buf = [0u16; 64];
    let len = unsafe { GetKeyNameTextW((scan << 16) | extended, buf.as_mut_ptr(), buf.len() as i32) };
    if len > 0 { from_wide(&buf[..len as usize]) } else { format!("#{code:X}") }
}

/// Например «Ctrl+Alt+Z».
pub fn describe(combo: Combo) -> String {
    let mut parts = Vec::new();
    for (bit, name) in [(mods::CONTROL, "Ctrl"), (mods::OPTION, "Alt"), (mods::SHIFT, "Shift"), (mods::COMMAND, "Win")]
    {
        if combo.mods & bit != 0 {
            parts.push(name.to_string());
        }
    }
    parts.push(key_name(combo.code));
    parts.join("+")
}
