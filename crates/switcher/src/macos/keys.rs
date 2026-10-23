//! Коды и названия клавиш macOS для горячих клавиш.

use engine::settings::{Combo, mods};

/// Функциональные клавиши F1–F15: годятся для сочетания и без модификаторов.
const FUNCTION_KEYS: &[(u16, &str)] = &[
    (122, "F1"), (120, "F2"), (99, "F3"), (118, "F4"), (96, "F5"), (97, "F6"), (98, "F7"), (100, "F8"),
    (101, "F9"), (109, "F10"), (103, "F11"), (111, "F12"), (105, "F13"), (107, "F14"), (113, "F15"),
];

/// Названия клавиш, у которых нет буквы.
const NAMED_KEYS: &[(u16, &str)] = &[
    (49, "Space"), (36, "↩"), (48, "⇥"), (53, "Esc"), (51, "⌫"), (117, "⌦"),
    (123, "←"), (124, "→"), (125, "↓"), (126, "↑"), (115, "Home"), (119, "End"), (116, "PgUp"), (121, "PgDn"),
];

pub const ESCAPE: u16 = 53;

pub fn is_function_key(code: u16) -> bool {
    FUNCTION_KEYS.iter().any(|(c, _)| *c == code)
}

/// Например «⌃⌥Z». `letter` — буква клавиши в английской раскладке.
pub fn describe(combo: Combo, letter: Option<char>) -> String {
    let mut s = String::new();
    for (bit, sign) in [(mods::CONTROL, '⌃'), (mods::OPTION, '⌥'), (mods::SHIFT, '⇧'), (mods::COMMAND, '⌘')] {
        if combo.mods & bit != 0 {
            s.push(sign);
        }
    }
    let name = FUNCTION_KEYS
        .iter()
        .chain(NAMED_KEYS)
        .find(|(c, _)| *c == combo.code)
        .map(|(_, n)| n.to_string())
        .or_else(|| letter.map(|c| c.to_uppercase().to_string()))
        .unwrap_or_else(|| format!("#{}", combo.code));
    s + &name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_combos() {
        assert_eq!(describe(Combo { code: 6, mods: mods::CONTROL | mods::OPTION }, Some('z')), "⌃⌥Z");
        assert_eq!(describe(Combo { code: 96, mods: 0 }, None), "F5");
        assert_eq!(describe(Combo { code: 49, mods: mods::COMMAND | mods::SHIFT }, Some(' ')), "⇧⌘Space");
    }
}
