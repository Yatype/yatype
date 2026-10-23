//! Перехват клавиатуры и мыши Windows (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`).
//!
//! Хуки вызываются в потоке, который их поставил, из его цикла сообщений.
//! Обработчик должен отвечать быстро: Windows отключает хук, который
//! долго думает.

use std::cell::{Cell, RefCell};

use engine::settings::mods;
use engine::{Combo, Input, Key, Pass};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CAPITAL, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU,
    VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_UP, SetWindowsHookExW, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN,
};

use super::inject;
use super::keymap::is_text_key;

type Handler = Box<dyn FnMut(Input) -> Pass>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
    /// Какие модификаторы сейчас зажаты, битами `mods`.
    static HELD: Cell<u8> = const { Cell::new(0) };
}

/// Ставит перехват. `handler` вызывается на каждое событие в этом потоке;
/// нужен цикл сообщений (`GetMessageW`).
pub fn install(handler: impl FnMut(Input) -> Pass + 'static) -> Result<(), &'static str> {
    HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), module, 0);
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0);
        if keyboard.is_null() || mouse.is_null() {
            return Err("Windows не дала поставить перехват клавиатуры");
        }
    }
    Ok(())
}

fn handle(input: Input) -> Pass {
    HANDLER.with(|h| match h.try_borrow_mut() {
        Ok(mut h) => h.as_mut().map_or(Pass::Through, |f| f(input)),
        Err(_) => Pass::Through,
    })
}

fn modifier_bit(vk: u16) -> Option<u8> {
    match vk {
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => Some(mods::SHIFT),
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => Some(mods::CONTROL),
        VK_MENU | VK_LMENU | VK_RMENU => Some(mods::OPTION),
        VK_LWIN | VK_RWIN => Some(mods::COMMAND),
        _ => None,
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }
    let event = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    // Наши собственные нажатия (замена текста) пропускаем как есть.
    if inject::is_ours(event) {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }
    // Маркеры и нажатия пользователя во время замены — через шлюз.
    if !inject::gate(event) {
        return 1;
    }
    if let Some(input) = classify(event) {
        if handle(input) == Pass::Swallow {
            return 1;
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// Событие клавиатуры → событие ядра. `None` — ядру неинтересно
/// (отпускание обычной клавиши, повтор зажатого модификатора).
fn classify(event: &KBDLLHOOKSTRUCT) -> Option<Input> {
    let vk = event.vkCode as u16;
    let up = event.flags & LLKHF_UP != 0;
    if let Some(bit) = modifier_bit(vk) {
        let before = HELD.get();
        let after = if up { before & !bit } else { before | bit };
        HELD.set(after);
        return (after != before).then_some(Input::Modifiers(after));
    }
    if up {
        return None;
    }
    let held = HELD.get();
    let scan = event.scanCode as u16;
    let extended = event.flags & LLKHF_EXTENDED != 0;
    // Расширенные клавиши (стрелки, Home…) отличаем от цифрового блока.
    let code = scan | if extended { 0x100 } else { 0 };
    if held & (mods::CONTROL | mods::OPTION | mods::COMMAND) != 0 {
        return Some(Input::Shortcut(Combo { code, mods: held }));
    }
    Some(match scan {
        0x39 => Input::Space,
        0x1C => Input::Enter,
        0x0E => Input::Backspace,
        _ if !extended && is_text_key(scan) => Input::Key(Key {
            code: scan,
            shift: held & mods::SHIFT != 0,
            caps: unsafe { GetKeyState(VK_CAPITAL as i32) } & 1 != 0,
        }),
        _ => Input::Shortcut(Combo { code, mods: held }),
    })
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && matches!(wparam as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
        // Клик мог переставить курсор — ядро забудет набранное.
        handle(Input::Reset);
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}
