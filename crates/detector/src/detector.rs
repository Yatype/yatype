//! Решение «оставить или перевести в другую раскладку».
//!
//! Для каждого слова сравниваем два прочтения тех же нажатий: как оно
//! набрано и как выглядело бы в другой раскладке. Сначала смотрим словари,
//! потом модель буквосочетаний. Затем по фразе целиком применяем правило
//! серии: сомнительные короткие слова рядом с уверенно неправильными
//! переводятся вместе с ними.

use crate::layout::{Lang, convert};
use crate::model::{LanguageData, normalize};

/// Насколько уверенно слово набрано не в той раскладке.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Слово точно в своей раскладке.
    Keep,
    /// Непонятно: короткое, редкое или осмысленное в обеих раскладках.
    Unsure,
    /// Скорее всего не та раскладка, но одного слова мало для решения.
    WeakSwitch,
    /// Точно не та раскладка.
    Switch,
}

#[derive(Clone, Debug)]
pub struct WordResult {
    /// Слово так, как оно набрано.
    pub typed: String,
    /// Те же нажатия в другой раскладке.
    pub converted: String,
    pub verdict: Verdict,
    /// Итоговое решение с учётом соседних слов.
    pub switch: bool,
    /// Короткое объяснение решения, для отладки.
    pub reason: &'static str,
}

/// Пороги. Подобраны на примерах из `testdata/`.
#[derive(Clone, Debug)]
pub struct Config {
    /// Насколько другое прочтение должно быть естественнее (в среднем
    /// логарифме вероятности), чтобы перевести слово уверенно.
    pub strong_margin: f64,
    /// То же для слабого решения.
    pub weak_margin: f64,
    /// Минимальная длина слова для уверенного решения без словаря.
    pub min_len_model: usize,
    /// Минимальная длина слова с опечаткой для уверенного решения: у
    /// коротких слов похожих соседей в словаре слишком много.
    pub min_len_typo: usize,
    /// Слова дальше этого места в частотном списке считаем ненадёжными:
    /// в хвосте списков много мусора.
    pub max_rank: u32,
    /// То же для слов из одной-двух букв, среди них мусора больше всего.
    pub max_rank_short: u32,
    /// Если оба прочтения есть в словарях, одно должно быть во столько раз
    /// частотнее другого.
    pub rank_ratio: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            strong_margin: 1.5,
            weak_margin: 0.7,
            min_len_model: 4,
            min_len_typo: 6,
            max_rank: 50_000,
            max_rank_short: 5_000,
            rank_ratio: 20,
        }
    }
}

pub struct Detector {
    en: LanguageData,
    ru: LanguageData,
    pub config: Config,
}

impl Detector {
    pub fn new(en: LanguageData, ru: LanguageData) -> Self {
        Detector { en, ru, config: Config::default() }
    }

    fn data(&self, lang: Lang) -> &LanguageData {
        match lang {
            Lang::En => &self.en,
            Lang::Ru => &self.ru,
        }
    }

    /// Оценивает одно слово, набранное в раскладке `layout`. Другое
    /// прочтение строится по встроенной таблице клавиш.
    pub fn analyze_word(&self, typed: &str, layout: Lang) -> WordResult {
        let converted = convert(typed, layout, layout.other());
        self.analyze_pair(typed, &converted, layout)
    }

    /// Оценивает слово, когда оба прочтения уже известны (например, их
    /// дала система по кодам клавиш).
    pub fn analyze_pair(&self, typed: &str, converted: &str, layout: Lang) -> WordResult {
        let (mut verdict, mut reason) = self.judge(typed, converted, layout);
        // Короткие слова заглавными — обычно аббревиатуры (РФ, МГУ), и
        // в другой раскладке они случайно совпадают с чем-то (HA, VUE).
        if is_abbreviation(strip_edges(typed, layout))
            && matches!(verdict, Verdict::Switch | Verdict::WeakSwitch)
        {
            verdict = Verdict::Unsure;
            reason = "похоже на аббревиатуру";
        }
        WordResult {
            typed: typed.to_string(),
            converted: converted.to_string(),
            verdict,
            switch: verdict == Verdict::Switch,
            reason,
        }
    }

    fn judge(&self, typed: &str, converted: &str, layout: Lang) -> (Verdict, &'static str) {
        let other = layout.other();
        let t_core = strip_edges(typed, layout);
        let o_core = strip_edges(converted, other);
        if t_core.is_empty() || o_core.is_empty() {
            return (Verdict::Keep, "нет букв");
        }
        if looks_like_code(t_core) {
            return (Verdict::Keep, "цифры, ссылка или код");
        }

        let t = normalize(t_core);
        let o = normalize(o_core);
        let len = o.chars().count();
        let cfg = &self.config;
        let limit = if len <= 2 { cfg.max_rank_short } else { cfg.max_rank };
        let t_rank = self.data(layout).rank(&t).filter(|r| *r <= limit);
        let o_rank = self.data(other).rank(&o).filter(|r| *r <= limit);
        let margin = self.data(other).model.score(&o) - self.data(layout).model.score(&t);

        match (t_rank, o_rank) {
            (Some(_), None) => (Verdict::Keep, "слово есть в словаре"),
            (None, Some(_)) if len >= 3 && margin > -cfg.weak_margin => {
                (Verdict::Switch, "другое прочтение есть в словаре")
            }
            (None, Some(_)) => (Verdict::WeakSwitch, "короткое слово из словаря"),
            (Some(tr), Some(or)) => {
                if or.saturating_mul(cfg.rank_ratio) < tr {
                    (Verdict::WeakSwitch, "оба в словаре, другое намного частотнее")
                } else if tr.saturating_mul(cfg.rank_ratio) < or {
                    (Verdict::Keep, "оба в словаре, набранное намного частотнее")
                } else {
                    (Verdict::Unsure, "оба в словаре")
                }
            }
            (None, None) => {
                if margin >= cfg.strong_margin && len >= cfg.min_len_model {
                    (Verdict::Switch, "буквосочетания другого языка")
                } else if len >= 3
                    && margin > -cfg.weak_margin
                    && self.data(other).has_near(&o)
                    && !self.data(layout).has_near(&t)
                {
                    // «blv» → «идм»: на одну опечатку от «идем».
                    if len >= cfg.min_len_typo && margin >= 0.0 {
                        (Verdict::Switch, "другое прочтение — слово с опечаткой")
                    } else {
                        (Verdict::WeakSwitch, "другое прочтение — слово с опечаткой")
                    }
                } else if margin >= cfg.weak_margin {
                    (Verdict::WeakSwitch, "буквосочетания скорее другого языка")
                } else if margin <= -cfg.weak_margin {
                    (Verdict::Keep, "буквосочетания своего языка")
                } else {
                    (Verdict::Unsure, "нет в словарях, модель не уверена")
                }
            }
        }
    }

    /// Оценивает фразу, набранную в раскладке `layout`, с правилом серии.
    pub fn analyze_text(&self, text: &str, layout: Lang) -> Vec<WordResult> {
        let mut words: Vec<WordResult> =
            text.split_whitespace().map(|w| self.analyze_word(w, layout)).collect();
        apply_series_rule(&mut words);
        words
    }
}

/// Сомнительные слова во фразе с уверенно неправильными тоже переводим.
///
/// Для каждого сомнительного слова смотрим на ближайшие уверенные слова
/// (`Switch` или `Keep`) слева и справа, пропуская сомнительные.
/// `WeakSwitch` переводится, если хоть с одной стороны `Switch`;
/// `Unsure` — если с одной стороны `Switch`, а с другой не `Keep`.
/// Два и больше `WeakSwitch` подряд (между ними только `Unsure`) тоже
/// считаются уверенной серией: «vs blv» → «мы идм».
pub fn apply_series_rule(words: &mut [WordResult]) {
    let mut start = 0;
    while start < words.len() {
        let end = (start..words.len())
            .find(|&j| !matches!(words[j].verdict, Verdict::WeakSwitch | Verdict::Unsure))
            .unwrap_or(words.len());
        let run = &mut words[start..end];
        if run.iter().filter(|w| w.verdict == Verdict::WeakSwitch).count() >= 2 {
            for w in run.iter_mut().filter(|w| w.verdict == Verdict::WeakSwitch) {
                w.verdict = Verdict::Switch;
                w.switch = true;
            }
        }
        start = end + 1;
    }

    let decided: Vec<Option<bool>> = words
        .iter()
        .map(|w| match w.verdict {
            Verdict::Switch => Some(true),
            Verdict::Keep => Some(false),
            _ => None,
        })
        .collect();
    for i in 0..words.len() {
        let left = decided[..i].iter().rev().find_map(|d| *d);
        let right = decided[i + 1..].iter().find_map(|d| *d);
        let near_switch = left == Some(true) || right == Some(true);
        let near_keep = left == Some(false) || right == Some(false);
        let flip = match words[i].verdict {
            Verdict::WeakSwitch => near_switch,
            Verdict::Unsure => near_switch && !near_keep,
            _ => false,
        };
        if flip {
            words[i].switch = true;
        }
    }
}

/// Отрезает по краям всё, что не является буквой языка: знаки препинания,
/// кавычки, скобки.
fn strip_edges(word: &str, lang: Lang) -> &str {
    word.trim_matches(|c: char| !lang.is_letter(c))
}

fn is_abbreviation(core: &str) -> bool {
    let n = core.chars().count();
    (2..=4).contains(&n) && core.chars().all(|c| c.is_uppercase())
}

fn looks_like_code(core: &str) -> bool {
    core.contains("://")
        || core.chars().any(|c| c.is_ascii_digit() || matches!(c, '@' | '_' | '\\' | '/' | '=' | '+'))
}
