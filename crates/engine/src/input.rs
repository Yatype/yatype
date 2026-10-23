//! События ввода, уже очищенные от подробностей системы.

use crate::settings::Combo;
use crate::typing::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// Клавиша основного блока, дающая символ.
    Key(Key),
    Space,
    Enter,
    Backspace,
    /// Изменились зажатые модификаторы (для двойного нажатия), битами `mods`.
    Modifiers(u8),
    /// Сочетание с Control/Option/Command или клавиша вне основного блока
    /// (стрелки, Tab, Esc, F1…). Может быть горячей клавишей; если нет —
    /// курсор мог переместиться.
    Shortcut(Combo),
    /// Курсор мог переместиться: клик мышью.
    Reset,
}

/// Что сделать с событием.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// Пропустить в приложение.
    Through,
    /// Не пропускать: программа сама отправила всё нужное.
    Swallow,
}
