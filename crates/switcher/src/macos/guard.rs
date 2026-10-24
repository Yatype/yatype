//! Где программе нельзя трогать ввод: поля паролей и приложения-исключения.
//!
//! Три слоя защиты:
//! 1. Защищённый ввод macOS (Secure Input). Его включают поля паролей,
//!    менеджеры паролей, «Безопасный ввод с клавиатуры» в Терминале. Пока он
//!    включён, система вообще не присылает нам нажатия.
//! 2. Поле в фокусе — поле пароля (`AXSecureTextField`), даже если
//!    защищённый ввод оно не включило.
//! 3. Активное приложение в списке исключений (или не в списке, где
//!    работать, если пользователь выбрал такой режим).

use engine::Settings;
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
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

/// Почему сейчас нельзя трогать ввод. `None` — можно.
pub fn blocked(settings: &Settings) -> Option<&'static str> {
    if unsafe { IsSecureEventInputEnabled() } != 0 {
        return Some("включён защищённый ввод");
    }
    if focused_is_password() {
        return Some("поле пароля");
    }
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let (id, name) = app_names(&app)?;
    if EXCLUDED_APPS.contains(&id.as_str()) {
        return Some("приложение в исключениях");
    }
    settings.app_blocked(&[&id, &name])
}

/// Идентификатор и название программы. У программы без идентификатора
/// вместо него — название.
fn app_names(app: &NSRunningApplication) -> Option<(String, String)> {
    let name = app.localizedName().map(|n| n.to_string());
    let id = app.bundleIdentifier().map(|i| i.to_string()).or_else(|| name.clone())?;
    Some((id.clone(), name.unwrap_or(id)))
}

/// Идентификатор активного приложения, например `com.apple.Notes`.
pub fn frontmost_app() -> Option<String> {
    frontmost_app_with_name().map(|(id, _)| id)
}

/// Идентификатор и название активного приложения. У программы без
/// идентификатора вместо него — название.
pub fn frontmost_app_with_name() -> Option<(String, String)> {
    app_names(&*NSWorkspace::sharedWorkspace().frontmostApplication()?)
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

