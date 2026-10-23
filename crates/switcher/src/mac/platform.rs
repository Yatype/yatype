//! macOS для ядра: раскладки, печать, защита паролей, значок в строке меню.

use detector::Lang;
use engine::{Combo, Key, Platform, Settings, Status};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSMenuItem, NSStatusItem};
use objc2_foundation::NSString;

use switcher::macos::keymap::{self, KeyLayout};
use switcher::macos::{guard, inject, keys, settings_store};

pub struct MacPlatform {
    pub en: KeyLayout,
    pub ru: KeyLayout,
    pub mtm: MainThreadMarker,
    pub status: Retained<NSStatusItem>,
    pub last: Retained<NSMenuItem>,
}

impl MacPlatform {
    fn layout(&self, lang: Lang) -> &KeyLayout {
        match lang {
            Lang::En => &self.en,
            Lang::Ru => &self.ru,
        }
    }
}

impl Platform for MacPlatform {
    fn current_lang(&mut self) -> Option<Lang> {
        keymap::current_lang()
    }

    fn read(&self, lang: Lang, keys: &[Key]) -> String {
        self.layout(lang).read(keys)
    }

    fn select(&mut self, lang: Lang) -> bool {
        self.layout(lang).select()
    }

    fn blocked(&mut self, user_excluded: &[String]) -> Option<&'static str> {
        guard::blocked(user_excluded)
    }

    fn replace(&mut self, delete: usize, insert: &str) {
        inject::batch(|| {
            inject::backspaces(delete);
            for line in insert.split_inclusive('\n') {
                match line.strip_suffix('\n') {
                    Some(text) => {
                        inject::type_text(text);
                        inject::press_return();
                    }
                    None => inject::type_text(line),
                }
            }
        });
    }

    fn is_function_key(&self, code: u16) -> bool {
        keys::is_function_key(code)
    }

    fn is_escape(&self, combo: Combo) -> bool {
        combo.mods == 0 && combo.code == keys::ESCAPE
    }

    fn describe(&self, combo: Combo) -> String {
        keys::describe(combo, self.en.char_for(&Key { code: combo.code, shift: false, caps: false }))
    }

    fn show_status(&mut self, status: Status) {
        let lang = match status.lang {
            Some(Lang::En) => "EN",
            Some(Lang::Ru) => "RU",
            None => "··",
        };
        let title = if !status.running {
            format!("⚠︎ {lang}")
        } else if !status.enabled {
            format!("{lang} ⏸")
        } else {
            lang.to_string()
        };
        if let Some(button) = self.status.button(self.mtm) {
            button.setTitle(&NSString::from_str(&title));
        }
    }

    fn show_last(&mut self, text: &str) {
        self.last.setTitle(&NSString::from_str(text));
        self.last.setHidden(false);
    }

    fn save_settings(&mut self, settings: &Settings) {
        settings_store::save(settings);
    }

    fn recording_finished(&mut self) {
        super::ui::refresh_soon();
    }
}
