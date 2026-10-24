//! Настройки программы: что пользователь выбрал. Где их хранить, решает
//! слой каждой системы.

/// Модификаторы сочетания клавиш, битами. На Windows `OPTION` — это Alt,
/// а `COMMAND` — клавиша Win.
pub mod mods {
    pub const CONTROL: u8 = 1;
    pub const OPTION: u8 = 2;
    pub const SHIFT: u8 = 4;
    pub const COMMAND: u8 = 8;
}

/// Сочетание клавиш: код клавиши (свой у каждой системы) и модификаторы.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combo {
    pub code: u16,
    pub mods: u8,
}

impl Combo {
    /// Можно ли назначить это горячей клавишей: нужен Control, Option (Alt)
    /// или Command (Win) — иначе сочетание мешало бы набору текста.
    /// Функциональные клавиши (`function_key`) можно и без модификаторов.
    pub fn is_valid_hotkey(self, function_key: bool) -> bool {
        self.mods & (mods::CONTROL | mods::OPTION | mods::COMMAND) != 0 || function_key
    }
}

/// Чем отменять исправление и переводить слово вручную.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hotkey {
    Control,
    Shift,
    Option,
    Command,
    /// Своё сочетание, см. `Settings::combo`.
    Combo,
}

impl Hotkey {
    /// В таком порядке варианты показываются в настройках.
    pub const ALL: [Hotkey; 5] = [Hotkey::Control, Hotkey::Shift, Hotkey::Option, Hotkey::Command, Hotkey::Combo];

    pub fn title(self) -> &'static str {
        match self {
            Hotkey::Control => "Двойной Control",
            Hotkey::Shift => "Двойной Shift",
            Hotkey::Option => "Двойной Option",
            Hotkey::Command => "Двойной Command",
            Hotkey::Combo => "Своё сочетание клавиш",
        }
    }

    /// Номер для хранения. Не менять: по нему читаются сохранённые настройки.
    pub fn stored(self) -> isize {
        match self {
            Hotkey::Control => 0,
            Hotkey::Shift => 1,
            Hotkey::Command => 2,
            Hotkey::Option => 3,
            Hotkey::Combo => 4,
        }
    }

    pub fn from_stored(n: isize) -> Hotkey {
        match n {
            1 => Hotkey::Shift,
            2 => Hotkey::Command,
            3 => Hotkey::Option,
            4 => Hotkey::Combo,
            _ => Hotkey::Control,
        }
    }

    /// Для двойного нажатия: (зажата ли наша клавиша, зажаты ли другие).
    /// `None` — это не двойное нажатие, а сочетание.
    pub fn double_tap_state(self, held: u8) -> Option<(bool, bool)> {
        let bit = match self {
            Hotkey::Control => mods::CONTROL,
            Hotkey::Shift => mods::SHIFT,
            Hotkey::Option => mods::OPTION,
            Hotkey::Command => mods::COMMAND,
            Hotkey::Combo => return None,
        };
        Some((held & bit != 0, held & !bit != 0))
    }
}

#[derive(Clone, Debug)]
pub struct Settings {
    /// Yatype работает. Если выключено — пауза: ни исправлений, ни горячей клавиши.
    pub enabled: bool,
    pub hotkey: Hotkey,
    /// Своё сочетание, если выбрано `Hotkey::Combo`.
    pub combo: Option<Combo>,
    /// Backspace сразу после исправления отменяет его (как в Punto Switcher).
    pub backspace_undo: bool,
    /// Приложения, где не исправлять: идентификатор (`ru.keepcoder.Telegram`),
    /// название («Telegram») или имя файла на Windows (`telegram.exe`).
    /// Терминалы и менеджеры паролей исключены всегда, см. `guard`.
    pub excluded_apps: Vec<String>,
    /// Работать только в программах из `allowed_apps`, а в остальных молчать.
    pub only_listed: bool,
    /// Программы, где работать, если включено `only_listed`.
    pub allowed_apps: Vec<String>,
    /// Слова, которые не исправлять: сюда попадают отменённые исправления.
    pub exception_words: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            hotkey: Hotkey::Control,
            combo: None,
            backspace_undo: false,
            excluded_apps: Vec::new(),
            only_listed: false,
            allowed_apps: Vec::new(),
            exception_words: Vec::new(),
        }
    }
}

/// Разбирает список «по одному в строке» из текстового поля.
pub fn parse_lines(text: &str) -> Vec<String> {
    let mut list: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    list.dedup();
    list
}

/// Есть ли приложение в списке исключений. `names` — как его можно назвать:
/// идентификатор, название, имя файла. Регистр и `.exe` не важны, чтобы
/// исключение срабатывало, как бы человек ни записал программу.
pub fn app_excluded(excluded: &[String], names: &[&str]) -> bool {
    let simple = |s: &str| {
        let s = s.trim().to_lowercase();
        s.strip_suffix(".exe").map(str::to_string).unwrap_or(s)
    };
    excluded.iter().any(|e| names.iter().any(|n| simple(e) == simple(n)))
}

impl Settings {
    /// Почему в этой программе нельзя исправлять по выбору пользователя.
    /// `names` — как программу можно назвать, см. [`app_excluded`].
    pub fn app_blocked(&self, names: &[&str]) -> Option<&'static str> {
        if self.only_listed {
            (!app_excluded(&self.allowed_apps, names)).then_some("программа не в списке, где работать")
        } else {
            app_excluded(&self.excluded_apps, names).then_some("приложение в исключениях")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_mode_blocks_everything_else() {
        let mut s = Settings { excluded_apps: vec!["Safari".into()], ..Settings::default() };
        assert!(s.app_blocked(&["com.apple.Safari", "Safari"]).is_some());
        assert!(s.app_blocked(&["ru.keepcoder.Telegram", "Telegram"]).is_none());
        s.only_listed = true;
        s.allowed_apps = vec!["Telegram".into()];
        assert!(s.app_blocked(&["ru.keepcoder.Telegram", "Telegram"]).is_none());
        // В этом режиме список исключений не действует: работает только список разрешённых.
        assert!(s.app_blocked(&["com.apple.Notes", "Заметки"]).is_some());
        assert!(s.app_blocked(&["com.apple.Safari", "Safari"]).is_some());
    }

    #[test]
    fn apps_match_by_any_name() {
        let list = vec!["Telegram".to_string(), "com.apple.Notes".to_string(), "WORD.EXE".to_string()];
        assert!(app_excluded(&list, &["ru.keepcoder.Telegram", "Telegram"]));
        assert!(app_excluded(&list, &["com.apple.notes", "Заметки"]));
        assert!(app_excluded(&list, &["winword.exe", "word"]));
        assert!(!app_excluded(&list, &["com.apple.Safari", "Safari"]));
        assert!(app_excluded(&["telegram".into()], &["telegram.exe"]));
    }

    #[test]
    fn parses_lines() {
        assert_eq!(parse_lines("  a \n\nb\nb\n"), vec!["a", "b"]);
    }

    #[test]
    fn hotkey_combos_need_a_modifier_or_function_key() {
        assert!(Combo { code: 6, mods: mods::CONTROL | mods::OPTION }.is_valid_hotkey(false));
        assert!(Combo { code: 96, mods: 0 }.is_valid_hotkey(true)); // F5
        assert!(!Combo { code: 6, mods: 0 }.is_valid_hotkey(false)); // просто Z
        assert!(!Combo { code: 6, mods: mods::SHIFT }.is_valid_hotkey(false)); // заглавная Z
    }

    #[test]
    fn double_tap_state_ignores_own_key() {
        let held = mods::CONTROL;
        assert_eq!(Hotkey::Control.double_tap_state(held), Some((true, false)));
        assert_eq!(Hotkey::Option.double_tap_state(held), Some((false, true)));
        assert_eq!(Hotkey::Combo.double_tap_state(held), None);
    }

    #[test]
    fn stored_numbers_round_trip() {
        for h in Hotkey::ALL {
            assert_eq!(Hotkey::from_stored(h.stored()), h);
        }
    }
}
