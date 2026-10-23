//! Что стереть и что впечатать, чтобы исправить фразу. Не зависит от платформы.

/// Одно слово фразы глазами исправителя.
#[derive(Clone, Debug)]
pub struct WordView {
    /// Как слово выглядит на экране.
    pub typed: String,
    /// Как оно выглядело бы в другой раскладке.
    pub converted: String,
    /// Сколько символов слово занимает на экране (по числу нажатий).
    pub len: usize,
    /// Пробелов после слова.
    pub spaces: usize,
    pub switch: bool,
}

/// Правка текста: стереть `delete` символов перед курсором и впечатать `insert`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub delete: usize,
    pub insert: String,
    /// Что было на месте стёртого — для отмены.
    pub original: String,
    /// С какого слова фразы начинается правка.
    pub first: usize,
}

impl Edit {
    /// Правка, которая возвращает всё как было.
    pub fn undo(&self) -> Edit {
        Edit {
            delete: self.insert.chars().count(),
            insert: self.original.clone(),
            original: self.insert.clone(),
            first: self.first,
        }
    }
}

/// Правка для фразы: от первого переводимого слова до конца. Слова между
/// ними, которые переводить не нужно, впечатываются как были.
pub fn plan(words: &[WordView]) -> Option<Edit> {
    let first = words.iter().position(|w| w.switch)?;
    let mut edit = Edit { delete: 0, insert: String::new(), original: String::new(), first };
    for w in &words[first..] {
        let spaces = " ".repeat(w.spaces);
        edit.delete += w.len + w.spaces;
        edit.insert += if w.switch { &w.converted } else { &w.typed };
        edit.insert += &spaces;
        edit.original += &w.typed;
        edit.original += &spaces;
    }
    Some(edit)
}

/// Ручной перевод одного слова (двойной Option без исправления для отмены).
pub fn convert_word(w: &WordView) -> Edit {
    let spaces = " ".repeat(w.spaces);
    Edit {
        delete: w.len + w.spaces,
        insert: format!("{}{spaces}", w.converted),
        original: format!("{}{spaces}", w.typed),
        first: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(typed: &str, converted: &str, spaces: usize, switch: bool) -> WordView {
        WordView {
            typed: typed.into(),
            converted: converted.into(),
            len: typed.chars().count(),
            spaces,
            switch,
        }
    }

    #[test]
    fn nothing_to_fix() {
        assert_eq!(plan(&[w("hello", "руддщ", 1, false), w("world", "цщкдв", 0, false)]), None);
    }

    #[test]
    fn fixes_from_first_wrong_word() {
        let e = plan(&[
            w("ok", "щл", 1, false),
            w("ghbdtn", "привет", 1, true),
            w("rfr", "как", 0, true),
        ])
        .unwrap();
        assert_eq!(e.first, 1);
        assert_eq!(e.delete, "ghbdtn rfr".len());
        assert_eq!(e.insert, "привет как");
        assert_eq!(e.original, "ghbdtn rfr");
    }

    #[test]
    fn keeps_unswitched_words_in_the_middle_and_spacing() {
        let e = plan(&[w("z", "я", 2, true), w("2024", "2024", 1, false), w("ujle", "году", 0, true)]).unwrap();
        assert_eq!(e.insert, "я  2024 году");
        assert_eq!(e.delete, "z  2024 ujle".chars().count());
    }

    #[test]
    fn undo_restores_original() {
        let e = plan(&[w("ghbdtn", "привет", 0, true)]).unwrap();
        let u = e.undo();
        assert_eq!(u.delete, "привет".chars().count());
        assert_eq!(u.insert, "ghbdtn");
    }

    #[test]
    fn manual_conversion_keeps_trailing_spaces() {
        let e = convert_word(&w("руддщ", "hello", 1, false));
        assert_eq!(e.delete, 6);
        assert_eq!(e.insert, "hello ");
    }
}
