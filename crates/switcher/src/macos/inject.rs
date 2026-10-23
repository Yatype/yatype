//! Отправка нажатий от имени пользователя: стереть символы, впечатать текст.
//!
//! Все свои события помечаем `MARK`, чтобы перехват их пропускал.
//! Текст вставляется как Unicode-строка, поэтому не зависит от раскладки.
//!
//! Замена — пачка из десятков событий; чтобы нажатия пользователя в неё не
//! вклинились, все события идут через шлюз из ядра (`engine::gate`).
//! Здесь — только маркеры и повторная отправка событий macOS.

use std::cell::RefCell;
use std::time::Instant;

use engine::gate::{Gate, Marker, Release};

use super::ffi::*;

/// Метка наших событий в поле `kCGEventSourceUserData`.
pub const MARK: i64 = 0x4C53_5754; // «LSWT»
/// Маркер конца пачки.
const MARK_END: i64 = MARK + 1;
/// Маркер конца отпущенных нажатий.
const MARK_BARRIER: i64 = MARK + 2;
/// Отпущенное (повторно отправленное) нажатие пользователя.
const MARK_REPLAY: i64 = MARK + 3;

thread_local! {
    /// Копии придержанных событий хранятся как адреса `CGEventRef`.
    static GATE: RefCell<Gate<usize>> = RefCell::new(Gate::default());
}

const BACKSPACE: u16 = 51;
const RETURN: u16 = 36;

fn mark_of(event: CGEventRef) -> i64 {
    unsafe { CGEventGetIntegerValueField(event, kCGEventSourceUserData) }
}

pub fn is_ours(event: CGEventRef) -> bool {
    mark_of(event) == MARK
}

unsafe fn post_marker(mark: i64) {
    // Маркер — «отпускание модификаторов»; перехват его дальше не пропустит.
    unsafe {
        let e = CGEventCreateKeyboardEvent(std::ptr::null(), 0, false);
        if !e.is_null() {
            CGEventSetType(e, kCGEventFlagsChanged);
            CGEventSetFlags(e, 0);
            CGEventSetIntegerValueField(e, kCGEventSourceUserData, mark);
            CGEventPost(kCGHIDEventTap, e);
            CFRelease(e);
        }
    }
}

/// Отправить пачку событий так, чтобы нажатия пользователя в неё не вклинились.
pub fn batch(send: impl FnOnce()) {
    GATE.with(|g| g.borrow_mut().begin_batch(Instant::now()));
    send();
    unsafe { post_marker(MARK_END) };
}

/// Что сделать с событием, которое пришло в перехват.
pub enum Gated {
    /// Маркер или придержанное событие: дальше не пропускать.
    Swallow,
    /// Обычное событие пользователя (в том числе отпущенное): обработать.
    Handle,
}

pub fn gate(event: CGEventRef) -> Gated {
    let marker = match mark_of(event) {
        MARK_END => Marker::End,
        MARK_BARRIER => Marker::Barrier,
        MARK_REPLAY => Marker::Replay,
        _ => Marker::User,
    };
    let decision = GATE.with(|g| {
        g.borrow_mut().on_event(marker, Instant::now(), || unsafe { CGEventCreateCopy(event) } as usize)
    });
    if let Some(release) = decision.release {
        replay(release);
    }
    if decision.handle { Gated::Handle } else { Gated::Swallow }
}

/// Отправить придержанные нажатия заново, по порядку.
fn replay(release: Release<usize>) {
    for e in release.events {
        let e = e as CGEventRef;
        if !e.is_null() {
            unsafe {
                CGEventSetIntegerValueField(e, kCGEventSourceUserData, MARK_REPLAY);
                CGEventPost(kCGHIDEventTap, e);
                CFRelease(e);
            }
        }
    }
    if release.barrier {
        unsafe { post_marker(MARK_BARRIER) };
    }
}

unsafe fn post_key(code: u16, text: Option<u16>) {
    for down in [true, false] {
        unsafe {
            let e = CGEventCreateKeyboardEvent(std::ptr::null(), code, down);
            if e.is_null() {
                return;
            }
            // Без модификаторов: если пользователь ещё держит Shift, он не
            // должен попасть в наши нажатия.
            CGEventSetFlags(e, 0);
            CGEventSetIntegerValueField(e, kCGEventSourceUserData, MARK);
            if let Some(unit) = text {
                CGEventKeyboardSetUnicodeString(e, 1, &unit);
            }
            CGEventPost(kCGHIDEventTap, e);
            CFRelease(e);
        }
    }
}

/// Стирает `n` символов перед курсором.
pub fn backspaces(n: usize) {
    for _ in 0..n {
        unsafe { post_key(BACKSPACE, None) };
    }
}

/// Впечатывает текст посимвольно (по одной кодовой единице UTF-16:
/// русский и английский в неё помещаются).
pub fn type_text(text: &str) {
    for unit in text.encode_utf16() {
        unsafe { post_key(0, Some(unit)) };
    }
}

pub fn press_return() {
    unsafe { post_key(RETURN, None) };
}
