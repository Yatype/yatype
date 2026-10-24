//! Сквозные сценарии ядра на «фейковой системе»: она имитирует текстовое
//! поле и две раскладки. Проверяет ту же логику, что и настоящий сквозной
//! тест на Mac, но работает на любой системе — в том числе в CI на Windows
//! и Linux.

use detector::{Lang, convert};
use engine::settings::mods;
use engine::{Combo, Engine, Hotkey, Input, Key, Pass, Platform, Settings, Status};

/// Клавиши основного блока: (EN, RU, EN с Shift, RU с Shift). Код клавиши —
/// её номер в списке.
const KEYS: &[(char, char, char, char)] = &[
    ('`', 'ё', '~', 'Ё'), ('1', '1', '!', '!'), ('2', '2', '@', '"'), ('3', '3', '#', '№'),
    ('4', '4', '$', ';'), ('5', '5', '%', '%'), ('6', '6', '^', ':'), ('7', '7', '&', '?'),
    ('8', '8', '*', '*'), ('9', '9', '(', '('), ('0', '0', ')', ')'), ('-', '-', '_', '_'), ('=', '=', '+', '+'),
    ('q', 'й', 'Q', 'Й'), ('w', 'ц', 'W', 'Ц'), ('e', 'у', 'E', 'У'), ('r', 'к', 'R', 'К'), ('t', 'е', 'T', 'Е'),
    ('y', 'н', 'Y', 'Н'), ('u', 'г', 'U', 'Г'), ('i', 'ш', 'I', 'Ш'), ('o', 'щ', 'O', 'Щ'), ('p', 'з', 'P', 'З'),
    ('[', 'х', '{', 'Х'), (']', 'ъ', '}', 'Ъ'),
    ('a', 'ф', 'A', 'Ф'), ('s', 'ы', 'S', 'Ы'), ('d', 'в', 'D', 'В'), ('f', 'а', 'F', 'А'), ('g', 'п', 'G', 'П'),
    ('h', 'р', 'H', 'Р'), ('j', 'о', 'J', 'О'), ('k', 'л', 'K', 'Л'), ('l', 'д', 'L', 'Д'), (';', 'ж', ':', 'Ж'),
    ('\'', 'э', '"', 'Э'),
    ('z', 'я', 'Z', 'Я'), ('x', 'ч', 'X', 'Ч'), ('c', 'с', 'C', 'С'), ('v', 'м', 'V', 'М'), ('b', 'и', 'B', 'И'),
    ('n', 'т', 'N', 'Т'), ('m', 'ь', 'M', 'Ь'), (',', 'б', '<', 'Б'), ('.', 'ю', '>', 'Ю'), ('/', '.', '?', ','),
];

const ESCAPE: u16 = 999;
const F5: u16 = 1005;

fn char_of(lang: Lang, key: Key) -> Option<char> {
    let (en, ru, en_up, ru_up) = *KEYS.get(key.code as usize)?;
    let upper = key.shift != (key.caps && en.is_alphabetic());
    Some(match (lang, upper) {
        (Lang::En, false) => en,
        (Lang::Ru, false) => ru,
        (Lang::En, true) => en_up,
        (Lang::Ru, true) => ru_up,
    })
}

/// Какую клавишу нажать, чтобы получить символ в раскладке.
fn key_for(lang: Lang, c: char) -> Key {
    for (i, &(en, ru, en_up, ru_up)) in KEYS.iter().enumerate() {
        let (low, up) = if lang == Lang::En { (en, en_up) } else { (ru, ru_up) };
        if c == low || c == up {
            return Key { code: i as u16, shift: c == up && c != low, caps: false };
        }
    }
    panic!("нет клавиши для «{c}» в {lang:?}");
}

#[derive(Default)]
struct Fake {
    lang: Option<Lang>,
    /// Содержимое текстового поля.
    text: String,
    password_field: bool,
    app: String,
    last: String,
    saved: Option<Settings>,
    status: Option<Status>,
}

impl Platform for Fake {
    fn current_lang(&mut self) -> Option<Lang> {
        self.lang
    }
    fn read(&self, lang: Lang, keys: &[Key]) -> String {
        keys.iter().filter_map(|k| char_of(lang, *k)).collect()
    }
    fn select(&mut self, lang: Lang) -> bool {
        self.lang = Some(lang);
        true
    }
    fn blocked(&mut self, settings: &Settings) -> Option<&'static str> {
        if self.password_field {
            Some("поле пароля")
        } else {
            settings.app_blocked(&[&self.app])
        }
    }
    fn replace(&mut self, delete: usize, insert: &str) {
        for _ in 0..delete {
            self.text.pop();
        }
        self.text.push_str(insert);
    }
    fn is_function_key(&self, code: u16) -> bool {
        code >= 1000
    }
    fn is_escape(&self, combo: Combo) -> bool {
        combo.code == ESCAPE && combo.mods == 0
    }
    fn describe(&self, combo: Combo) -> String {
        format!("{}+{}", combo.mods, combo.code)
    }
    fn show_status(&mut self, status: Status) {
        self.status = Some(status);
    }
    fn show_last(&mut self, text: &str) {
        self.last = text.to_string();
    }
    fn save_settings(&mut self, settings: &Settings) {
        self.saved = Some(settings.clone());
    }
}

fn engine(lang: Lang, settings: Settings) -> Engine<Fake> {
    let mut e = Engine::new(Fake { lang: Some(lang), ..Fake::default() }, settings, false, false);
    e.running = true;
    e
}

/// Отдать событие движку и, если он его пропустил, применить к полю так,
/// как это сделало бы приложение.
fn send(e: &mut Engine<Fake>, input: Input) {
    if e.on_input(input) == Pass::Swallow {
        return;
    }
    let lang = e.platform.lang.expect("раскладка");
    let f = &mut e.platform;
    match input {
        Input::Key(k) => f.text.extend(char_of(lang, k)),
        Input::Space => f.text.push(' '),
        Input::Enter => f.text.push('\n'),
        Input::Backspace => {
            f.text.pop();
        }
        Input::Modifiers(_) | Input::Shortcut(_) | Input::Reset | Input::Held => {}
    }
}

/// Нажать клавиши, которые в раскладке `physical` дают этот текст. Это
/// физические клавиши: если программа переключит раскладку, те же клавиши
/// дальше дадут буквы новой раскладки — как у живого человека.
fn press(e: &mut Engine<Fake>, physical: Lang, text: &str) {
    for c in text.chars() {
        let input = match c {
            ' ' => Input::Space,
            '\n' => Input::Enter,
            '\u{8}' => Input::Backspace,
            _ => Input::Key(key_for(physical, c)),
        };
        send(e, input);
    }
}

fn double_control(e: &mut Engine<Fake>) {
    for _ in 0..2 {
        send(e, Input::Modifiers(mods::CONTROL));
        send(e, Input::Modifiers(0));
    }
}

#[test]
fn forgotten_layout_is_fixed_and_switched() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "ghbdtn rfr ltkf ");
    assert_eq!(e.platform.text, "привет как дела ");
    assert_eq!(e.platform.lang, Some(Lang::Ru));
}

#[test]
fn english_typed_in_russian_layout() {
    let mut e = engine(Lang::Ru, Settings::default());
    press(&mut e, Lang::Ru, "руддщ цщкдв ");
    assert_eq!(e.platform.text, "hello world ");
    assert_eq!(e.platform.lang, Some(Lang::En));
}

#[test]
fn correct_text_is_untouched() {
    let mut e = engine(Lang::Ru, Settings::default());
    press(&mut e, Lang::Ru, "Привет, как дела? ");
    assert_eq!(e.platform.text, "Привет, как дела? ");
    assert_eq!(e.platform.lang, Some(Lang::Ru));
}

#[test]
fn enter_sends_fixed_text() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "ghbdtn\n");
    assert_eq!(e.platform.text, "привет\n");
}

#[test]
fn short_words_are_fixed_together_with_the_series() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "z nt,z k.,k. ");
    assert_eq!(e.platform.text, "я тебя люблю ");
}

#[test]
fn backspace_while_typing_is_tracked() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "ghbdtt\u{8}n ");
    assert_eq!(e.platform.text, "привет ");
}

#[test]
fn hotkey_undoes_the_fix_and_remembers_the_word() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "ckjdj ");
    assert_eq!(e.platform.text, "слово ");
    double_control(&mut e);
    assert_eq!(e.platform.text, "ckjdj ");
    assert_eq!(e.platform.lang, Some(Lang::En));
    assert!(e.platform.saved.as_ref().unwrap().exception_words.contains(&"ckjdj".to_string()));
    // Второй раз то же слово уже не трогаем.
    press(&mut e, Lang::En, "ckjdj ");
    assert_eq!(e.platform.text, "ckjdj ckjdj ");
}

#[test]
fn hotkey_converts_the_last_word_by_hand() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "lf ");
    assert_eq!(e.platform.text, "lf ", "одно короткое слово само не исправляется");
    double_control(&mut e);
    assert_eq!(e.platform.text, "да ");
    assert_eq!(e.platform.lang, Some(Lang::Ru));
}

#[test]
fn backspace_right_after_fix_undoes_it_when_enabled() {
    let settings = Settings { backspace_undo: true, ..Settings::default() };
    let mut e = engine(Lang::En, settings);
    press(&mut e, Lang::En, "pfdnhf \u{8}");
    assert_eq!(e.platform.text, "pfdnhf ");
    assert_eq!(e.platform.lang, Some(Lang::En));
}

#[test]
fn backspace_just_deletes_when_undo_is_off() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "pfdnhf \u{8}");
    assert_eq!(e.platform.text, "завтра");
}

#[test]
fn other_double_tap_keys() {
    let settings = Settings { hotkey: Hotkey::Shift, ..Settings::default() };
    let mut e = engine(Lang::En, settings);
    press(&mut e, Lang::En, "ckjdj ");
    // Двойной Control теперь ничего не делает…
    double_control(&mut e);
    assert_eq!(e.platform.text, "слово ");
    // …а двойной Shift отменяет.
    for _ in 0..2 {
        send(&mut e, Input::Modifiers(mods::SHIFT));
        send(&mut e, Input::Modifiers(0));
    }
    assert_eq!(e.platform.text, "ckjdj ");
}

#[test]
fn recorded_combo_becomes_the_hotkey() {
    let mut e = engine(Lang::Ru, Settings::default());
    let combo = Combo { code: 6, mods: mods::CONTROL | mods::OPTION };
    e.recording = true;
    // Буква без модификатора не годится — ждём дальше.
    assert_eq!(e.on_input(Input::Shortcut(Combo { code: 6, mods: 0 })), Pass::Swallow);
    assert!(e.recording);
    e.on_input(Input::Shortcut(combo));
    assert!(!e.recording);
    assert_eq!(e.settings.hotkey, Hotkey::Combo);
    assert_eq!(e.settings.combo, Some(combo));

    press(&mut e, Lang::Ru, "руддщ");
    send(&mut e, Input::Shortcut(combo));
    assert_eq!(e.platform.text, "hello");
}

#[test]
fn function_key_can_be_recorded_without_modifiers_and_escape_cancels() {
    let mut e = engine(Lang::Ru, Settings::default());
    e.recording = true;
    e.on_input(Input::Shortcut(Combo { code: ESCAPE, mods: 0 }));
    assert!(!e.recording);
    assert_eq!(e.settings.combo, None);

    e.recording = true;
    e.on_input(Input::Shortcut(Combo { code: F5, mods: 0 }));
    assert_eq!(e.settings.combo, Some(Combo { code: F5, mods: 0 }));
}

#[test]
fn password_fields_are_never_touched() {
    let mut e = engine(Lang::En, Settings::default());
    e.platform.password_field = true;
    press(&mut e, Lang::En, "ghbdtn ");
    assert_eq!(e.platform.text, "ghbdtn ");
    double_control(&mut e);
    assert_eq!(e.platform.text, "ghbdtn ");
}

#[test]
fn excluded_apps_are_never_touched() {
    let settings = Settings { excluded_apps: vec!["terminal".into()], ..Settings::default() };
    let mut e = engine(Lang::En, settings);
    e.platform.app = "terminal".into();
    press(&mut e, Lang::En, "ghbdtn ");
    assert_eq!(e.platform.text, "ghbdtn ");
}

#[test]
fn only_listed_apps_are_fixed() {
    let settings = Settings { only_listed: true, allowed_apps: vec!["telegram".into()], ..Settings::default() };
    let mut e = engine(Lang::En, settings);
    e.platform.app = "game".into();
    press(&mut e, Lang::En, "ghbdtn ");
    assert_eq!(e.platform.text, "ghbdtn ");
    e.platform.app = "telegram".into();
    press(&mut e, Lang::En, "ghbdtn ");
    assert_eq!(e.platform.text, "ghbdtn привет ");
}

/// Одна буква без соседей не исправляется, а со следующим словом — да.
#[test]
fn lone_letter_waits_for_the_next_word() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "e ");
    assert_eq!(e.platform.text, "e ");
    assert_eq!(e.platform.lang, Some(Lang::En));
    press(&mut e, Lang::En, "ntcn ");
    assert_eq!(e.platform.text, "у тест ");
}

/// В играх клавиши держат: «e» с автоповтором и прыжок пробелом — не слово.
#[test]
fn held_key_is_not_a_word() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "e");
    send(&mut e, Input::Held);
    press(&mut e, Lang::En, " ");
    assert_eq!(e.platform.text, "e ");
    assert_eq!(e.platform.lang, Some(Lang::En));
}

#[test]
fn paused_does_nothing_even_on_hotkey() {
    let settings = Settings { enabled: false, ..Settings::default() };
    let mut e = engine(Lang::En, settings);
    press(&mut e, Lang::En, "ghbdtn ");
    assert_eq!(e.platform.text, "ghbdtn ");
    double_control(&mut e);
    assert_eq!(e.platform.text, "ghbdtn ");
}

#[test]
fn click_forgets_typed_words() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "z ");
    send(&mut e, Input::Reset);
    press(&mut e, Lang::En, "ok ");
    // «z» до клика не трогаем: курсор мог уйти в другое место.
    assert_eq!(e.platform.text, "z ok ");
}

#[test]
fn status_reflects_language_and_pause() {
    let mut e = engine(Lang::En, Settings::default());
    press(&mut e, Lang::En, "ghbdtn ");
    e.refresh_status();
    assert_eq!(e.platform.status, Some(Status { lang: Some(Lang::Ru), running: true, enabled: true }));
    e.settings.enabled = false;
    e.settings_changed();
    assert_eq!(e.platform.status.unwrap().enabled, false);
}

#[test]
fn all_test_phrases_through_the_engine() {
    for (lang, text) in [
        (Lang::Ru, include_str!("../../detector/testdata/ru.txt")),
        (Lang::En, include_str!("../../detector/testdata/en.txt")),
    ] {
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            // Забыл переключить раскладку.
            let mut e = engine(lang.other(), Settings::default());
            press(&mut e, lang.other(), &format!("{} ", convert(line, lang, lang.other())));
            assert_eq!(e.platform.text, format!("{line} "), "в чужой раскладке");
            // Правильная раскладка.
            let mut e = engine(lang, Settings::default());
            press(&mut e, lang, &format!("{line} "));
            assert_eq!(e.platform.text, format!("{line} "), "в своей раскладке");
        }
    }
}
