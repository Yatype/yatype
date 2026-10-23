//! Где программе нельзя трогать ввод: поля паролей и программы-исключения.
//!
//! - Поле пароля: стандартное поле Windows со стилем `ES_PASSWORD` или
//!   любое поле, о котором программа сообщает через UI Automation, что это
//!   пароль (так делают браузеры), см. `uia`.
//! - Программы, где ничего не исправляем всегда: терминалы (там `sudo`,
//!   `ssh` и т. п. спрашивают пароль без звёздочек), менеджеры паролей,
//!   удалённый рабочий стол.

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ES_PASSWORD, GUITHREADINFO, GWL_STYLE, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongW,
    GetWindowThreadProcessId,
};

use super::from_wide;

/// Программы (имя файла, строчными), где ничего не исправляем.
const EXCLUDED_APPS: &[&str] = &[
    // Терминалы
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "windowsterminal.exe",
    "wt.exe",
    "conhost.exe",
    "openconsole.exe",
    "putty.exe",
    "kitty.exe",
    "mintty.exe",
    "alacritty.exe",
    "wezterm-gui.exe",
    "mobaxterm.exe",
    "termius.exe",
    // Пароли и ключи
    "keepass.exe",
    "keepassxc.exe",
    "1password.exe",
    "bitwarden.exe",
    "enpass.exe",
    "dashlane.exe",
    "lastpass.exe",
    // Удалённый рабочий стол: нажатия уходят на другой компьютер
    "mstsc.exe",
];

/// Активная программа: имя файла строчными (`telegram.exe`) и название для
/// меню (`Telegram`).
pub fn foreground_app() -> Option<(String, String)> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = from_wide(&buf[..len as usize]);
        let file = path.rsplit('\\').next()?.to_lowercase();
        let name = path.rsplit('\\').next()?.trim_end_matches(".exe").trim_end_matches(".EXE").to_string();
        Some((file, name))
    }
}

pub fn always_excluded(file: &str) -> bool {
    EXCLUDED_APPS.contains(&file)
}

fn focused_is_password() -> bool {
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(thread, &mut info) == 0 || info.hwndFocus.is_null() {
            return false;
        }
        let mut class = [0u16; 64];
        let len = GetClassNameW(info.hwndFocus, class.as_mut_ptr(), class.len() as i32);
        let class = from_wide(&class[..len.max(0) as usize]).to_lowercase();
        let style = GetWindowLongW(info.hwndFocus, GWL_STYLE) as u32;
        class == "edit" && style & ES_PASSWORD as u32 != 0
    }
}

/// Почему сейчас нельзя трогать ввод. `None` — можно. `user_excluded` —
/// программы, которые пользователь исключил сам (имена файлов).
pub fn blocked(user_excluded: &[String]) -> Option<&'static str> {
    if focused_is_password() || super::uia::password_focused() {
        return Some("поле пароля");
    }
    if let Some((file, _)) = foreground_app() {
        if always_excluded(&file) || user_excluded.contains(&file) {
            return Some("приложение в исключениях");
        }
    }
    None
}
