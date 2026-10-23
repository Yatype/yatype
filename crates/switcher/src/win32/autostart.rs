//! Запуск при входе в Windows: значение в разделе реестра
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW,
};

use super::wide;

const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const NAME: &str = "Yatype";

fn open(access: u32) -> Option<HKEY> {
    let mut key: HKEY = std::ptr::null_mut();
    let err = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, wide(RUN).as_ptr(), 0, access, &mut key) };
    (err == 0).then_some(key)
}

pub fn enabled() -> bool {
    let Some(key) = open(KEY_QUERY_VALUE) else { return false };
    let err = unsafe {
        RegQueryValueExW(
            key,
            wide(NAME).as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    unsafe { RegCloseKey(key) };
    err == 0
}

pub fn set(on: bool) -> Result<(), String> {
    let key = open(KEY_SET_VALUE).ok_or("нет доступа к разделу автозапуска")?;
    let err = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = wide(&format!("\"{}\"", exe.display()));
        unsafe {
            RegSetValueExW(key, wide(NAME).as_ptr(), 0, REG_SZ, value.as_ptr().cast(), (value.len() * 2) as u32)
        }
    } else {
        unsafe { RegDeleteValueW(key, wide(NAME).as_ptr()) }
    };
    unsafe { RegCloseKey(key) };
    if err == 0 || !on { Ok(()) } else { Err(format!("ошибка реестра {err}")) }
}
