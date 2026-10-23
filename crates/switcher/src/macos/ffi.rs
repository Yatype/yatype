//! Объявления системных функций macOS, которых нет в objc2:
//! CoreFoundation, перехват событий CoreGraphics и раскладки из Carbon.

#![allow(non_upper_case_globals, non_snake_case, dead_code)]

use std::ffi::{c_char, c_void};

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFArrayRef = *const c_void;
pub type CFDataRef = *const c_void;
pub type CFMachPortRef = *mut c_void;
pub type CFRunLoopSourceRef = *mut c_void;
pub type CFRunLoopRef = *mut c_void;
pub type TISInputSourceRef = *mut c_void;
pub type CGEventRef = *mut c_void;
pub type CGEventTapProxy = *mut c_void;
pub type Boolean = u8;

pub const kCFStringEncodingUTF8: u32 = 0x0800_0100;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub static kCFRunLoopCommonModes: CFStringRef;

    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    pub fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> Boolean;
    pub fn CFArrayGetCount(array: CFArrayRef) -> isize;
    pub fn CFArrayGetValueAtIndex(array: CFArrayRef, index: isize) -> *const c_void;
    pub fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: isize, encoding: u32) -> Boolean;
    pub fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
    pub fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: CFMachPortRef,
        order: isize,
    ) -> CFRunLoopSourceRef;
    pub fn CFRunLoopGetMain() -> CFRunLoopRef;
    pub fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFAbsoluteTimeGetCurrent() -> f64;
    pub fn CFRunLoopTimerCreate(
        allocator: *const c_void,
        fire_date: f64,
        interval: f64,
        flags: usize,
        order: isize,
        callout: extern "C" fn(timer: *mut c_void, info: *mut c_void),
        context: *const c_void,
    ) -> *mut c_void;
    pub fn CFRunLoopAddTimer(rl: CFRunLoopRef, timer: *mut c_void, mode: CFStringRef);
}

// Типы событий.
pub const kCGEventLeftMouseDown: u32 = 1;
pub const kCGEventRightMouseDown: u32 = 3;
pub const kCGEventKeyDown: u32 = 10;
pub const kCGEventKeyUp: u32 = 11;
pub const kCGEventFlagsChanged: u32 = 12;
pub const kCGEventOtherMouseDown: u32 = 25;
pub const kCGEventTapDisabledByTimeout: u32 = 0xFFFF_FFFE;
pub const kCGEventTapDisabledByUserInput: u32 = 0xFFFF_FFFF;

// Поля и флаги.
pub const kCGKeyboardEventKeycode: u32 = 9;
pub const kCGEventSourceUserData: u32 = 42;
pub const kCGEventFlagMaskAlphaShift: u64 = 0x0001_0000;
pub const kCGEventFlagMaskShift: u64 = 0x0002_0000;
pub const kCGEventFlagMaskControl: u64 = 0x0004_0000;
pub const kCGEventFlagMaskAlternate: u64 = 0x0008_0000;
pub const kCGEventFlagMaskCommand: u64 = 0x0010_0000;

pub const kCGHIDEventTap: u32 = 0;
pub const kCGSessionEventTap: u32 = 1;
pub const kCGHeadInsertEventTap: u32 = 0;
pub const kCGEventTapOptionDefault: u32 = 0;
pub const kCGEventTapOptionListenOnly: u32 = 1;

pub type CGEventTapCallBack =
    extern "C" fn(proxy: CGEventTapProxy, ty: u32, event: CGEventRef, user_info: *mut c_void) -> CGEventRef;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    pub fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    pub fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    pub fn CGEventGetFlags(event: CGEventRef) -> u64;
    pub fn CGPreflightListenEventAccess() -> bool;
    pub fn CGRequestListenEventAccess() -> bool;
    pub fn CGEventCreateKeyboardEvent(source: *const c_void, keycode: u16, key_down: bool) -> CGEventRef;
    pub fn CGEventKeyboardSetUnicodeString(event: CGEventRef, length: usize, string: *const u16);
    pub fn CGEventSetFlags(event: CGEventRef, flags: u64);
    pub fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    pub fn CGEventPost(tap: u32, event: CGEventRef);
    pub fn CGEventSetType(event: CGEventRef, ty: u32);
    pub fn CGEventCreateCopy(event: CGEventRef) -> CGEventRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    pub static kAXTrustedCheckOptionPrompt: CFStringRef;
    pub fn AXIsProcessTrusted() -> Boolean;
    pub fn AXIsProcessTrustedWithOptions(options: *const c_void) -> Boolean;
    pub fn AXUIElementCreateSystemWide() -> CFTypeRef;
    pub fn AXUIElementCopyAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: *mut CFTypeRef) -> i32;
}

pub const kUCKeyActionDown: u16 = 0;
pub const kUCKeyTranslateNoDeadKeysMask: u32 = 1;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    pub static kTISPropertyInputSourceID: CFStringRef;
    pub static kTISPropertyInputSourceType: CFStringRef;
    pub static kTISPropertyInputSourceLanguages: CFStringRef;
    pub static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    pub static kTISTypeKeyboardLayout: CFStringRef;

    pub fn TISCopyCurrentKeyboardLayoutInputSource() -> TISInputSourceRef;
    pub fn TISCreateInputSourceList(properties: *const c_void, include_all_installed: Boolean) -> CFArrayRef;
    pub fn TISGetInputSourceProperty(source: TISInputSourceRef, key: CFStringRef) -> *const c_void;
    pub fn TISSelectInputSource(source: TISInputSourceRef) -> i32;
    pub fn LMGetKbdType() -> u8;
    pub fn IsSecureEventInputEnabled() -> Boolean;
    pub fn UCKeyTranslate(
        layout: *const c_void,
        virtual_key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        key_translate_options: u32,
        dead_key_state: *mut u32,
        max_string_length: usize,
        actual_string_length: *mut usize,
        unicode_string: *mut u16,
    ) -> i32;
}

/// Копирует CFString в String.
pub unsafe fn cf_string(s: CFStringRef) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let mut buf = [0 as c_char; 256];
    let ok = unsafe { CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as isize, kCFStringEncodingUTF8) };
    if ok == 0 {
        return None;
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    Some(cstr.to_string_lossy().into_owned())
}
