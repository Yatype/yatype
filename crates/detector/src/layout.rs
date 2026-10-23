//! Соответствие клавиш между раскладками US QWERTY и «Русская — ПК» (ЙЦУКЕН).
//!
//! Это запасная таблица для тестов и для случаев, когда система не может
//! подсказать раскладку. В приложении буквы для каждой клавиши берутся
//! у самой системы (на macOS — `UCKeyTranslate`), так что поддерживаются
//! любые установленные раскладки.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    En,
    Ru,
}

impl Lang {
    pub fn other(self) -> Lang {
        match self {
            Lang::En => Lang::Ru,
            Lang::Ru => Lang::En,
        }
    }

    /// Буква ли это в алфавите языка (без учёта регистра).
    pub fn is_letter(self, c: char) -> bool {
        let c = lower(c);
        match self {
            Lang::En => c.is_ascii_lowercase(),
            Lang::Ru => ('а'..='я').contains(&c) || c == 'ё',
        }
    }
}

/// Пары «символ в EN — символ в RU» для одной и той же клавиши.
const KEYS: &[(char, char)] = &[
    ('q', 'й'), ('w', 'ц'), ('e', 'у'), ('r', 'к'), ('t', 'е'), ('y', 'н'),
    ('u', 'г'), ('i', 'ш'), ('o', 'щ'), ('p', 'з'), ('[', 'х'), (']', 'ъ'),
    ('a', 'ф'), ('s', 'ы'), ('d', 'в'), ('f', 'а'), ('g', 'п'), ('h', 'р'),
    ('j', 'о'), ('k', 'л'), ('l', 'д'), (';', 'ж'), ('\'', 'э'),
    ('z', 'я'), ('x', 'ч'), ('c', 'с'), ('v', 'м'), ('b', 'и'), ('n', 'т'),
    ('m', 'ь'), (',', 'б'), ('.', 'ю'), ('/', '.'), ('`', 'ё'),
    // С Shift.
    ('{', 'Х'), ('}', 'Ъ'), (':', 'Ж'), ('"', 'Э'), ('<', 'Б'), ('>', 'Ю'),
    ('?', ','), ('~', 'Ё'), ('@', '"'), ('#', '№'), ('$', ';'), ('^', ':'),
    ('&', '?'),
];

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn map_char(c: char, from: Lang, to: Lang) -> char {
    if from == to {
        return c;
    }
    for &(en, ru) in KEYS {
        let (src, dst) = if from == Lang::En { (en, ru) } else { (ru, en) };
        if c == src {
            return dst;
        }
        // Заглавные буквы: ищем строчную пару и поднимаем регистр.
        if c.is_uppercase() && lower(c) == src && src.is_alphabetic() && dst.is_alphabetic() {
            return dst.to_uppercase().next().unwrap_or(dst);
        }
    }
    c
}

/// Что получилось бы, если бы те же клавиши нажали в раскладке `to`.
pub fn convert(text: &str, from: Lang, to: Lang) -> String {
    text.chars().map(|c| map_char(c, from, to)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn en_to_ru() {
        assert_eq!(convert("ghbdtn", Lang::En, Lang::Ru), "привет");
        assert_eq!(convert("Ghbdtn&", Lang::En, Lang::Ru), "Привет?");
        assert_eq!(convert("rfr ltkf?", Lang::En, Lang::Ru), "как дела,");
        assert_eq!(convert("[jhjij", Lang::En, Lang::Ru), "хорошо");
    }

    #[test]
    fn ru_to_en() {
        assert_eq!(convert("руддщ", Lang::Ru, Lang::En), "hello");
        assert_eq!(convert("Вщтэе", Lang::Ru, Lang::En), "Don't");
    }

    #[test]
    fn round_trip() {
        let s = "Съешь же ещё этих мягких французских булок, да выпей чаю.";
        assert_eq!(convert(&convert(s, Lang::Ru, Lang::En), Lang::En, Lang::Ru), s);
    }
}
