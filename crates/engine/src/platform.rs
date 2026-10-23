//! Что ядру нужно от операционной системы. Каждая система (macOS, Windows,
//! в тестах — фейковая) реализует этот интерфейс по-своему.

use detector::Lang;

use crate::settings::{Combo, Settings};
use crate::typing::Key;

/// Что показать в значке программы.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// Текущая раскладка; `None` — не русская и не английская.
    pub lang: Option<Lang>,
    /// Перехват клавиатуры работает (есть разрешение).
    pub running: bool,
    /// Автоисправление включено.
    pub enabled: bool,
}

pub trait Platform {
    /// Раскладка, которая сейчас выбрана.
    fn current_lang(&mut self) -> Option<Lang>;
    /// Текст, который дали бы эти нажатия в раскладке `lang`.
    fn read(&self, lang: Lang, keys: &[Key]) -> String;
    /// Сделать раскладку текущей. `false` — не получилось.
    fn select(&mut self, lang: Lang) -> bool;
    /// Почему сейчас нельзя трогать ввод (поле пароля, приложение в
    /// исключениях). `None` — можно.
    fn blocked(&mut self, user_excluded: &[String]) -> Option<&'static str>;
    /// Стереть `delete` символов перед курсором и впечатать `insert` одной
    /// пачкой, в которую не вклинятся нажатия пользователя. `'\n'` — Enter.
    fn replace(&mut self, delete: usize, insert: &str);

    /// Функциональная клавиша (F1…): её можно назначить и без модификаторов.
    fn is_function_key(&self, code: u16) -> bool;
    /// Esc без модификаторов — отмена записи сочетания.
    fn is_escape(&self, combo: Combo) -> bool;
    /// Как показать сочетание пользователю, например «⌃⌥Z» или «Ctrl+Alt+Z».
    fn describe(&self, combo: Combo) -> String;

    fn show_status(&mut self, status: Status);
    /// Последнее исправление — для меню.
    fn show_last(&mut self, text: &str);
    fn save_settings(&mut self, settings: &Settings);
    /// Запись горячей клавиши закончилась: обновить интерфейс. Вызывается из
    /// обработки нажатия, поэтому обновлять стоит чуть позже.
    fn recording_finished(&mut self) {}
}
