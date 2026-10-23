//! Перехват клавиатуры и мыши через `CGEventTap`.
//!
//! Перехват активный: обработчик может «придержать» нажатие (пробел или
//! Enter), пока программа исправляет слово, и отправить его сам. Остальные
//! события проходят без изменений. Колбэк вызывается в главном потоке,
//! потому что источник событий добавлен в главный run loop.

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use objc2_foundation::{NSDictionary, NSNumber, NSString};

use super::ffi::*;
use super::inject;
use engine::settings::mods;
use engine::{Combo, Input, Key, Pass};

fn mods_of(flags: u64) -> u8 {
    let mut m = 0;
    for (mask, bit) in [
        (kCGEventFlagMaskControl, mods::CONTROL),
        (kCGEventFlagMaskAlternate, mods::OPTION),
        (kCGEventFlagMaskShift, mods::SHIFT),
        (kCGEventFlagMaskCommand, mods::COMMAND),
    ] {
        if flags & mask != 0 {
            m |= bit;
        }
    }
    m
}

type Handler = Box<dyn FnMut(Input) -> Pass>;

static TAP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Есть ли разрешение «Универсальный доступ» (нужно, чтобы менять ввод).
pub fn has_permission() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Попросить разрешение: macOS покажет системный запрос.
pub fn request_permission() {
    unsafe {
        let key: &NSString = &*kAXTrustedCheckOptionPrompt.cast::<NSString>();
        let options = NSDictionary::from_slices(&[key], &[&*NSNumber::new_bool(true)]);
        AXIsProcessTrustedWithOptions((&*options as *const NSDictionary<NSString, NSNumber>).cast());
    }
}

/// Ставит перехват. `handler` вызывается на каждое событие в главном потоке.
pub fn install(handler: impl FnMut(Input) -> Pass + 'static) -> Result<(), &'static str> {
    let handler: *mut Handler = Box::into_raw(Box::new(Box::new(handler)));
    let mask = (1u64 << kCGEventKeyDown)
        | (1u64 << kCGEventFlagsChanged)
        | (1u64 << kCGEventLeftMouseDown)
        | (1u64 << kCGEventRightMouseDown)
        | (1u64 << kCGEventOtherMouseDown);
    unsafe {
        let tap = CGEventTapCreate(
            kCGSessionEventTap,
            kCGHeadInsertEventTap,
            kCGEventTapOptionDefault,
            mask,
            callback,
            handler.cast(),
        );
        if tap.is_null() {
            drop(Box::from_raw(handler));
            return Err("система не дала поставить перехват клавиатуры (нет разрешения?)");
        }
        TAP.store(tap, Ordering::SeqCst);
        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
        CGEventTapEnable(tap, true);
    }
    Ok(())
}

#[allow(non_upper_case_globals)] // константы macOS в образцах match
extern "C" fn callback(_proxy: CGEventTapProxy, ty: u32, event: CGEventRef, user_info: *mut c_void) -> CGEventRef {
    // Система отключает перехват, если колбэк долго отвечал. Включаем обратно.
    if ty == kCGEventTapDisabledByTimeout || ty == kCGEventTapDisabledByUserInput {
        let tap = TAP.load(Ordering::SeqCst);
        if !tap.is_null() {
            unsafe { CGEventTapEnable(tap, true) };
        }
        return event;
    }
    // Наши собственные нажатия (замена текста) пропускаем как есть.
    if inject::is_ours(event) {
        return event;
    }
    // Маркеры и нажатия пользователя во время замены — через шлюз.
    if let inject::Gated::Swallow = inject::gate(event) {
        return std::ptr::null_mut();
    }
    let handler = unsafe { &mut *(user_info as *mut Handler) };
    let input = match ty {
        kCGEventKeyDown => unsafe { classify_key(event) },
        kCGEventFlagsChanged => {
            let flags = unsafe { CGEventGetFlags(event) };
            Input::Modifiers(mods_of(flags))
        }
        _ => Input::Reset, // нажатие кнопки мыши
    };
    match handler(input) {
        Pass::Through => event,
        Pass::Swallow => std::ptr::null_mut(),
    }
}

unsafe fn classify_key(event: CGEventRef) -> Input {
    let code = unsafe { CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode) } as u16;
    let flags = unsafe { CGEventGetFlags(event) };
    let held = mods_of(flags);
    if held & (mods::CONTROL | mods::OPTION | mods::COMMAND) != 0 {
        return Input::Shortcut(Combo { code, mods: held });
    }
    match code {
        49 => Input::Space,
        36 | 76 => Input::Enter,
        51 => Input::Backspace,
        // Tab, Esc и все прочие клавиши вне основного блока (стрелки,
        // Home/End, F1…F15, цифровой блок).
        48 | 53 => Input::Shortcut(Combo { code, mods: held }),
        0..=50 => Input::Key(Key {
            code,
            shift: flags & kCGEventFlagMaskShift != 0,
            caps: flags & kCGEventFlagMaskAlphaShift != 0,
        }),
        _ => Input::Shortcut(Combo { code, mods: held }),
    }
}
