//! Windows для ядра: раскладки, печать, защита паролей, значок в трее.

use std::time::SystemTime;

use detector::Lang;
use engine::{Combo, Key, Platform, Settings, Status};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

use switcher::win32::keymap::{self, KeyLayout};
use switcher::win32::{guard, inject, keys, settings_store};

use super::tray;

pub struct WinPlatform {
    pub en: KeyLayout,
    pub ru: KeyLayout,
    /// Скрытое окно значка в трее: ему приходят сообщения меню.
    pub hwnd: HWND,
    /// Последнее исправление — для меню.
    pub last: String,
    /// Программа, в которой пользователь последний раз набирал слово:
    /// (имя файла, название) — для пункта «Не исправлять в …».
    pub last_app: Option<(String, String)>,
    /// Когда мы сами последний раз записали файл настроек.
    pub saved_at: Option<SystemTime>,
}

impl WinPlatform {
    fn layout(&self, lang: Lang) -> &KeyLayout {
        match lang {
            Lang::En => &self.en,
            Lang::Ru => &self.ru,
        }
    }
}

impl Platform for WinPlatform {
    fn current_lang(&mut self) -> Option<Lang> {
        keymap::current_lang()
    }

    fn read(&self, lang: Lang, keys: &[Key]) -> String {
        self.layout(lang).read(keys)
    }

    fn select(&mut self, lang: Lang) -> bool {
        self.layout(lang).select()
    }

    fn blocked(&mut self, settings: &Settings) -> Option<&'static str> {
        if let Some(app) = guard::foreground_app() {
            if app.0 != "explorer.exe" {
                self.last_app = Some(app);
            }
        }
        guard::blocked(settings)
    }

    fn replace(&mut self, delete: usize, insert: &str) {
        inject::replace(delete, insert);
    }

    fn is_function_key(&self, code: u16) -> bool {
        keys::is_function_key(code)
    }

    fn is_escape(&self, combo: Combo) -> bool {
        combo.mods == 0 && combo.code == keys::ESCAPE
    }

    fn describe(&self, combo: Combo) -> String {
        keys::describe(combo)
    }

    fn show_status(&mut self, status: Status) {
        tray::set_status(self.hwnd, status);
    }

    fn show_last(&mut self, text: &str) {
        self.last = text.to_string();
    }

    fn save_settings(&mut self, settings: &Settings) {
        settings_store::save(settings);
        self.saved_at = settings_store::modified();
    }

    fn recording_finished(&mut self) {
        // Подсказку покажем, когда обработка нажатия закончится.
        unsafe { PostMessageW(self.hwnd, tray::WM_RECORDED, 0, 0) };
    }
}
