//! Что пользователь набрал подряд с последнего сброса.
//!
//! Храним не буквы, а нажатые клавиши: из них можно получить текст в любой
//! раскладке. Пробелы считаем точно — чтобы при замене стереть ровно столько
//! символов, сколько на экране. Не зависит от платформы.

/// Одно нажатие клавиши, дающей символ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    /// Аппаратный код клавиши (на macOS — virtual keycode).
    pub code: u16,
    pub shift: bool,
    pub caps: bool,
}

/// Законченное слово и пробелы после него.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub keys: Vec<Key>,
    pub spaces: usize,
}

/// Сколько последних слов помнить для правила серии.
const MAX_WORDS: usize = 8;

#[derive(Default)]
pub struct Typing {
    /// Законченные слова фразы, по порядку.
    words: Vec<Word>,
    /// Слово, которое набирается сейчас.
    current: Vec<Key>,
}

impl Typing {
    pub fn key(&mut self, key: Key) {
        self.current.push(key);
    }

    /// Backspace. Если текущее слово пустое, стирается пробел, а когда
    /// пробелов не осталось — продолжаем редактировать предыдущее слово.
    pub fn backspace(&mut self) {
        if self.current.pop().is_some() {
            return;
        }
        if let Some(last) = self.words.last_mut() {
            last.spaces -= 1;
            if last.spaces == 0 {
                self.current = self.words.pop().expect("есть").keys;
            }
        }
    }

    /// Пробел после текущего слова (или ещё один пробел после предыдущего).
    pub fn space(&mut self) {
        if !self.current.is_empty() {
            self.words.push(Word { keys: std::mem::take(&mut self.current), spaces: 1 });
            if self.words.len() > MAX_WORDS {
                self.words.remove(0);
            }
        } else if let Some(last) = self.words.last_mut() {
            last.spaces += 1;
        }
        // Пробелы в самом начале не запоминаем: стирать их не придётся.
    }

    pub fn words(&self) -> &[Word] {
        &self.words
    }

    pub fn current(&self) -> &[Key] {
        &self.current
    }

    /// Забыть первые `n` законченных слов: они остались как есть, и после
    /// смены раскладки их уже нельзя читать в новой.
    pub fn drop_first(&mut self, n: usize) {
        self.words.drain(..n.min(self.words.len()));
    }

    /// Оставить только последнее слово (текущее или последнее законченное).
    pub fn keep_last_word(&mut self) {
        if !self.current.is_empty() {
            self.words.clear();
        } else {
            let n = self.words.len().saturating_sub(1);
            self.drop_first(n);
        }
    }

    /// Курсор мог уйти куда угодно (клик, стрелки, сочетание клавиш,
    /// смена раскладки). Всё, что помнили, больше не соответствует тексту.
    pub fn reset(&mut self) {
        self.words.clear();
        self.current.clear();
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.current.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: u16) -> Key {
        Key { code, shift: false, caps: false }
    }

    fn shape(t: &Typing) -> (Vec<(Vec<u16>, usize)>, Vec<u16>) {
        (
            t.words().iter().map(|w| (w.keys.iter().map(|k| k.code).collect(), w.spaces)).collect(),
            t.current().iter().map(|k| k.code).collect(),
        )
    }

    #[test]
    fn collects_words_and_spaces() {
        let mut t = Typing::default();
        t.key(k(1));
        t.key(k(2));
        t.space();
        t.space();
        t.key(k(3));
        assert_eq!(shape(&t), (vec![(vec![1, 2], 2)], vec![3]));
    }

    #[test]
    fn leading_spaces_are_ignored() {
        let mut t = Typing::default();
        t.space();
        t.key(k(1));
        assert_eq!(shape(&t), (vec![], vec![1]));
    }

    #[test]
    fn backspace_inside_word_and_across_spaces() {
        let mut t = Typing::default();
        t.key(k(1));
        t.key(k(2));
        t.space();
        t.space();
        t.key(k(3));
        t.backspace(); // «3»
        t.backspace(); // второй пробел
        assert_eq!(shape(&t), (vec![(vec![1, 2], 1)], vec![]));
        t.backspace(); // первый пробел — снова внутри слова
        assert_eq!(shape(&t), (vec![], vec![1, 2]));
        t.backspace(); // «2»
        t.key(k(4));
        assert_eq!(shape(&t), (vec![], vec![1, 4]));
    }

    #[test]
    fn backspace_on_empty_does_nothing() {
        let mut t = Typing::default();
        t.backspace();
        assert!(t.is_empty());
    }

    #[test]
    fn reset_forgets_everything() {
        let mut t = Typing::default();
        t.key(k(1));
        t.space();
        t.key(k(2));
        t.reset();
        assert!(t.is_empty());
    }

    #[test]
    fn keeps_only_recent_words() {
        let mut t = Typing::default();
        for i in 0..20 {
            t.key(k(i));
            t.space();
        }
        assert_eq!(t.words().len(), MAX_WORDS);
        assert_eq!(t.words().last().unwrap().keys[0].code, 19);
    }

    #[test]
    fn drop_and_keep_last() {
        let mut t = Typing::default();
        for i in 0..3 {
            t.key(k(i));
            t.space();
        }
        t.drop_first(1);
        assert_eq!(t.words().len(), 2);
        t.keep_last_word();
        assert_eq!(shape(&t), (vec![(vec![2], 1)], vec![]));
        t.key(k(9));
        t.keep_last_word();
        assert_eq!(shape(&t), (vec![], vec![9]));
    }
}
