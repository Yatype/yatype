//! Печать от имени пользователя через `SendInput`: стереть символы,
//! впечатать текст. Текст вставляется Unicode-символами, поэтому не
//! зависит от раскладки.
//!
//! Нажатия пользователя во время замены придерживает шлюз из ядра
//! (`engine::gate`); здесь — маркеры и повторная отправка событий Windows.
//! Свои события помечаем в `dwExtraInfo`.

use std::cell::RefCell;
use std::time::Instant;

use engine::gate::{Gate, Marker, Release};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, SendInput, VK_BACK, VK_NONAME, VK_RETURN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_UP};

/// Метка наших событий.
pub const MARK: usize = 0x4C53_5754; // «LSWT»
const MARK_END: usize = MARK + 1;
const MARK_BARRIER: usize = MARK + 2;
const MARK_REPLAY: usize = MARK + 3;

/// Придержанное нажатие пользователя.
#[derive(Clone, Copy, Debug)]
pub struct Held {
    vk: u16,
    scan: u16,
    flags: KEYBD_EVENT_FLAGS,
}

thread_local! {
    static GATE: RefCell<Gate<Held>> = RefCell::new(Gate::default());
}

pub fn is_ours(event: &KBDLLHOOKSTRUCT) -> bool {
    event.dwExtraInfo == MARK
}

fn key(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS, extra: usize) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: extra } },
    }
}

fn tap(events: &mut Vec<INPUT>, vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) {
    events.push(key(vk, scan, flags, MARK));
    events.push(key(vk, scan, flags | KEYEVENTF_KEYUP, MARK));
}

fn send(events: &[INPUT]) {
    if !events.is_empty() {
        unsafe { SendInput(events.len() as u32, events.as_ptr(), size_of::<INPUT>() as i32) };
    }
}

/// Маркер — отпускание несуществующей клавиши; перехват его дальше не пропустит.
fn marker(mark: usize) -> INPUT {
    key(VK_NONAME, 0, KEYEVENTF_KEYUP, mark)
}

/// Стереть `delete` символов и впечатать `insert` (`'\n'` — Enter) одной
/// пачкой. `SendInput` вставляет её в поток ввода целиком, а шлюз держит
/// нажатия пользователя, пока пачка не пройдёт.
pub fn replace(delete: usize, insert: &str) {
    let mut events = Vec::new();
    for _ in 0..delete {
        tap(&mut events, VK_BACK, 0, 0);
    }
    for unit in insert.encode_utf16() {
        if unit == '\n' as u16 {
            tap(&mut events, VK_RETURN, 0, 0);
        } else {
            tap(&mut events, 0, unit, KEYEVENTF_UNICODE);
        }
    }
    events.push(marker(MARK_END));
    GATE.with(|g| g.borrow_mut().begin_batch(Instant::now()));
    send(&events);
}

/// Пропустить событие через шлюз. `true` — обработать как обычно,
/// `false` — не пропускать дальше (маркер или придержанное нажатие).
pub fn gate(event: &KBDLLHOOKSTRUCT) -> bool {
    let marker = match event.dwExtraInfo {
        MARK_END => Marker::End,
        MARK_BARRIER => Marker::Barrier,
        MARK_REPLAY => Marker::Replay,
        _ => Marker::User,
    };
    let held = Held {
        vk: event.vkCode as u16,
        scan: event.scanCode as u16,
        flags: (if event.flags & LLKHF_UP != 0 { KEYEVENTF_KEYUP } else { 0 })
            | (if event.flags & LLKHF_EXTENDED != 0 { KEYEVENTF_EXTENDEDKEY } else { 0 }),
    };
    let decision = GATE.with(|g| g.borrow_mut().on_event(marker, Instant::now(), || held));
    if let Some(release) = decision.release {
        replay(release);
    }
    decision.handle
}

/// Отправить придержанные нажатия заново, по порядку.
fn replay(release: Release<Held>) {
    let mut events: Vec<INPUT> = release.events.iter().map(|h| key(h.vk, h.scan, h.flags, MARK_REPLAY)).collect();
    if release.barrier {
        events.push(marker(MARK_BARRIER));
    }
    send(&events);
}
