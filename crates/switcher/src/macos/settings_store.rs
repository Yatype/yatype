//! Хранение настроек в системных `UserDefaults`, как у обычных программ для Mac.

use engine::settings::{Combo, Hotkey, Settings};
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSString, NSUserDefaults};

const ENABLED: &str = "enabled";
const HOTKEY: &str = "hotkey";
const COMBO_CODE: &str = "comboCode";
const COMBO_MODS: &str = "comboMods";
const BACKSPACE_UNDO: &str = "backspaceUndo";
const EXCLUDED_APPS: &str = "excludedApps";
const EXCEPTION_WORDS: &str = "exceptionWords";
const ONLY_LISTED: &str = "onlyListedApps";
const ALLOWED_APPS: &str = "allowedApps";

fn key(name: &str) -> objc2::rc::Retained<NSString> {
    NSString::from_str(name)
}

fn load_list(d: &NSUserDefaults, name: &str) -> Vec<String> {
    d.stringArrayForKey(&key(name))
        .map(|a| a.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

fn save_list(d: &NSUserDefaults, name: &str, list: &[String]) {
    let items: Vec<_> = list.iter().map(|s| NSString::from_str(s)).collect();
    let array = NSArray::from_retained_slice(&items);
    let object: &AnyObject = &array;
    unsafe { d.setObject_forKey(Some(object), &key(name)) };
}

pub fn load() -> Settings {
    let d = NSUserDefaults::standardUserDefaults();
    let combo = d.objectForKey(&key(COMBO_CODE)).map(|_| Combo {
        code: d.integerForKey(&key(COMBO_CODE)) as u16,
        mods: d.integerForKey(&key(COMBO_MODS)) as u8,
    });
    Settings {
        // Если настройку ещё не сохраняли — включено.
        enabled: d.objectForKey(&key(ENABLED)).is_none() || d.boolForKey(&key(ENABLED)),
        hotkey: Hotkey::from_stored(d.integerForKey(&key(HOTKEY))),
        combo,
        backspace_undo: d.boolForKey(&key(BACKSPACE_UNDO)),
        excluded_apps: load_list(&d, EXCLUDED_APPS),
        only_listed: d.boolForKey(&key(ONLY_LISTED)),
        allowed_apps: load_list(&d, ALLOWED_APPS),
        exception_words: load_list(&d, EXCEPTION_WORDS),
    }
}

pub fn save(settings: &Settings) {
    let d = NSUserDefaults::standardUserDefaults();
    d.setBool_forKey(settings.enabled, &key(ENABLED));
    d.setInteger_forKey(settings.hotkey.stored(), &key(HOTKEY));
    if let Some(c) = settings.combo {
        d.setInteger_forKey(c.code as isize, &key(COMBO_CODE));
        d.setInteger_forKey(c.mods as isize, &key(COMBO_MODS));
    }
    d.setBool_forKey(settings.backspace_undo, &key(BACKSPACE_UNDO));
    save_list(&d, EXCLUDED_APPS, &settings.excluded_apps);
    d.setBool_forKey(settings.only_listed, &key(ONLY_LISTED));
    save_list(&d, ALLOWED_APPS, &settings.allowed_apps);
    save_list(&d, EXCEPTION_WORDS, &settings.exception_words);
}
