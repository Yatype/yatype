//! Раскладки Windows: какой символ даёт каждая клавиша.
//!
//! Клавиша задаётся скан-кодом — это физическое место на клавиатуре, оно не
//! зависит от раскладки. Таблицы символов строим один раз через
//! `ToUnicodeEx` для каждой установленной раскладки.

use std::collections::HashMap;

use detector::Lang;
use engine::Key;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, GetKeyboardLayoutList, HKL, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW, ToUnicodeEx, VK_CAPITAL,
    VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, WM_INPUTLANGCHANGEREQUEST,
};

/// Скан-коды основного блока клавиатуры, дающие символы.
pub fn is_text_key(scan: u16) -> bool {
    matches!(scan, 0x02..=0x0D | 0x10..=0x1B | 0x1E..=0x29 | 0x2B..=0x35 | 0x56)
}

pub struct KeyLayout {
    pub hkl: HKL,
    /// (скан-код, Shift, Caps Lock) → символ.
    table: HashMap<(u16, bool, bool), char>,
}

impl KeyLayout {
    fn new(hkl: HKL) -> KeyLayout {
        let mut table = HashMap::new();
        for scan in (0x02u16..=0x56).filter(|s| is_text_key(*s)) {
            let vk = unsafe { MapVirtualKeyExW(scan as u32, MAPVK_VSC_TO_VK_EX, hkl) };
            if vk == 0 {
                continue;
            }
            for shift in [false, true] {
                for caps in [false, true] {
                    let mut state = [0u8; 256];
                    if shift {
                        state[VK_SHIFT as usize] = 0x80;
                    }
                    if caps {
                        state[VK_CAPITAL as usize] = 0x01;
                    }
                    let mut buf = [0u16; 8];
                    // Флаг 4: не менять состояние клавиатуры (мёртвые клавиши и т. п.).
                    let n = unsafe {
                        ToUnicodeEx(vk, scan as u32, state.as_ptr(), buf.as_mut_ptr(), buf.len() as i32, 4, hkl)
                    };
                    if n == 1 {
                        if let Some(c) = char::from_u32(buf[0] as u32).filter(|c| !c.is_control()) {
                            table.insert((scan, shift, caps), c);
                        }
                    }
                }
            }
        }
        KeyLayout { hkl, table }
    }

    pub fn char_for(&self, key: &Key) -> Option<char> {
        self.table.get(&(key.code, key.shift, key.caps)).copied()
    }

    /// Текст, который дали бы эти нажатия в этой раскладке.
    pub fn read(&self, keys: &[Key]) -> String {
        keys.iter().filter_map(|k| self.char_for(k)).collect()
    }

    /// Сделать раскладку текущей в активном окне.
    pub fn select(&self) -> bool {
        unsafe {
            let window = GetForegroundWindow();
            !window.is_null() && PostMessageW(window, WM_INPUTLANGCHANGEREQUEST, 0, self.hkl as isize) != 0
        }
    }
}

/// Язык раскладки: младшее слово HKL — код языка, его младшие 10 бит —
/// основной язык.
fn lang_of(hkl: HKL) -> Option<Lang> {
    match (hkl as usize) & 0x3FF {
        0x09 => Some(Lang::En),
        0x19 => Some(Lang::Ru),
        _ => None,
    }
}

/// Первая установленная раскладка для каждого языка.
pub fn installed_layouts() -> (Option<KeyLayout>, Option<KeyLayout>) {
    let count = unsafe { GetKeyboardLayoutList(0, std::ptr::null_mut()) };
    let mut list: Vec<HKL> = vec![std::ptr::null_mut(); count.max(0) as usize];
    let got = unsafe { GetKeyboardLayoutList(count, list.as_mut_ptr()) };
    list.truncate(got.max(0) as usize);
    let (mut en, mut ru) = (None, None);
    for hkl in list {
        let slot = match lang_of(hkl) {
            Some(Lang::En) => &mut en,
            Some(Lang::Ru) => &mut ru,
            None => continue,
        };
        if slot.is_none() {
            *slot = Some(KeyLayout::new(hkl));
        }
    }
    (en, ru)
}

/// Язык раскладки активного окна.
pub fn current_lang() -> Option<Lang> {
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        lang_of(GetKeyboardLayout(thread))
    }
}
