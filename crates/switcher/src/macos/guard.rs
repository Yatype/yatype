//! Где программе нельзя трогать ввод: поля паролей и приложения-исключения.
//!
//! Три слоя защиты:
//! 1. Защищённый ввод macOS (Secure Input). Его включают поля паролей,
//!    менеджеры паролей, «Безопасный ввод с клавиатуры» в Терминале. Пока он
//!    включён, система вообще не присылает нам нажатия.
//! 2. Поле в фокусе — поле пароля (`AXSecureTextField`), даже если
//!    защищённый ввод оно не включило.
//! 3. Активное приложение в списке исключений.

use objc2_app_kit::NSWorkspace;
use objc2_foundation::NSString;

use super::ffi::*;

/// Приложения, где ничего не исправляем: в терминалах `sudo` и `ssh`
/// спрашивают пароль без звёздочек, в менеджерах паролей вводят пароли.
const EXCLUDED_APPS: &[&str] = &[
    // Терминалы
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "dev.warp.Warp-Stable",
    "net.kovidgoyal.kitty",
    "org.alacritty",
    "com.mitchellh.ghostty",
    "co.zeit.hyper",
    "com.github.wez.wezterm",
    // Пароли и ключи
    "com.apple.keychainaccess",
    "com.apple.Passwords",
    "com.1password.1password",
    "com.agilebits.onepassword7",
    "com.bitwarden.desktop",
    "in.sinew.Enpass-Desktop",
    "com.lastpass.LastPass",
    "org.keepassxc.keepassxc",
    "com.dashlane.dashlanephonefinal",
];

/// Почему сейчас нельзя трогать ввод. `None` — можно. `user_excluded` —
/// приложения, которые пользователь исключил сам.
pub fn blocked(user_excluded: &[String]) -> Option<&'static str> {
    if unsafe { IsSecureEventInputEnabled() } != 0 {
        return Some("включён защищённый ввод");
    }
    if focused_is_password() {
        return Some("поле пароля");
    }
    if let Some(id) = frontmost_app() {
        if EXCLUDED_APPS.contains(&id.as_str()) || user_excluded.contains(&id) {
            return Some("приложение в исключениях");
        }
    }
    None
}

/// Идентификатор активного приложения, например `com.apple.Notes`.
pub fn frontmost_app() -> Option<String> {
    frontmost_app_with_name().map(|(id, _)| id)
}

/// Идентификатор и название активного приложения.
pub fn frontmost_app_with_name() -> Option<(String, String)> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let id = app.bundleIdentifier()?.to_string();
    let name = app.localizedName().map(|n| n.to_string()).unwrap_or_else(|| id.clone());
    Some((id, name))
}

/// Исключено ли приложение всегда (терминалы, менеджеры паролей).
pub fn always_excluded(id: &str) -> bool {
    EXCLUDED_APPS.contains(&id)
}

/// Строковый атрибут элемента интерфейса (роль, подроль).
unsafe fn copy_attribute(element: CFTypeRef, name: &str) -> CFTypeRef {
    let name = NSString::from_str(name);
    let mut value: CFTypeRef = std::ptr::null();
    let err = unsafe {
        AXUIElementCopyAttributeValue(element, (&*name as *const NSString).cast(), &mut value)
    };
    if err != 0 { std::ptr::null() } else { value }
}

fn focused_is_password() -> bool {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return false;
        }
        let focused = copy_attribute(system, "AXFocusedUIElement");
        CFRelease(system);
        if focused.is_null() {
            return false;
        }
        let subrole = copy_attribute(focused, "AXSubrole");
        CFRelease(focused);
        if subrole.is_null() {
            return false;
        }
        let is_secure = cf_string(subrole).as_deref() == Some("AXSecureTextField");
        CFRelease(subrole);
        is_secure
    }
}
