//! Языковые данные: словарь словоформ и модель буквосочетаний.
//!
//! Словарь — все формы слов в сжатом виде (`fst`), плюс собственный список
//! частых слов проекта (`data/core/`), который даёт словам ранг частоты.
//! Модель буквосочетаний — вероятности троек букв (триграмм). Она ловит
//! «невозможные» сочетания вроде «ghbd» в английском или «щщ» в русском
//! даже для слов, которых нет в словаре.

use std::collections::{BTreeSet, HashMap};

use crate::layout::Lang;

const START: char = '^';
const END: char = '$';

/// Ранг для слов, которые есть в словаре, но не в списке частых:
/// «обычное слово». Частые слова из `data/core/` идут с рангами 1, 2, 3…
pub const DICT_RANK: u32 = 30_000;

/// Во сколько раз частые слова весят больше остальных при обучении модели.
const CORE_WEIGHT: f64 = 5.0;

/// Приводит слово к виду, в котором оно хранится: строчные буквы, «ё» → «е».
pub fn normalize(word: &str) -> String {
    word.chars()
        .flat_map(char::to_lowercase)
        .map(|c| if c == 'ё' { 'е' } else { c })
        .collect()
}

/// Интерполированная модель триграмм символов.
pub struct CharModel {
    tri: HashMap<[char; 3], f64>,
    tri_ctx: HashMap<[char; 2], f64>,
    bi: HashMap<[char; 2], f64>,
    bi_ctx: HashMap<char, f64>,
    uni: HashMap<char, f64>,
    total: f64,
    alphabet: f64,
}

impl CharModel {
    fn train<'a>(words: impl Iterator<Item = (&'a str, f64)>) -> Self {
        let mut tri = HashMap::new();
        for (word, weight) in words {
            for t in padded(word).windows(3) {
                *tri.entry([t[0], t[1], t[2]]).or_default() += weight;
            }
        }
        Self::from_counts(tri)
    }

    /// Остальные счётчики выводятся из триграмм, поэтому хранить нужно только их.
    fn from_counts(tri: HashMap<[char; 3], f64>) -> Self {
        let mut m = CharModel {
            tri: HashMap::new(),
            tri_ctx: HashMap::new(),
            bi: HashMap::new(),
            bi_ctx: HashMap::new(),
            uni: HashMap::new(),
            total: 0.0,
            alphabet: 0.0,
        };
        for (&[a, b, c], &w) in &tri {
            *m.tri_ctx.entry([a, b]).or_default() += w;
            *m.bi.entry([b, c]).or_default() += w;
            *m.bi_ctx.entry(b).or_default() += w;
            *m.uni.entry(c).or_default() += w;
            m.total += w;
        }
        m.tri = tri;
        m.alphabet = m.uni.len() as f64;
        m
    }

    /// Строки «abc<TAB>счётчик».
    pub fn to_text(&self) -> String {
        let mut lines: Vec<String> =
            self.tri.iter().map(|(t, w)| format!("{}{}{}\t{:.1}", t[0], t[1], t[2], w)).collect();
        lines.sort();
        lines.join("\n")
    }

    pub fn from_text(text: &str) -> Self {
        let mut tri = HashMap::new();
        for line in text.lines() {
            let Some((key, count)) = line.split_once('\t') else { continue };
            let chars: Vec<char> = key.chars().collect();
            if let ([a, b, c], Ok(w)) = (chars.as_slice(), count.parse::<f64>()) {
                tri.insert([*a, *b, *c], w);
            }
        }
        Self::from_counts(tri)
    }

    fn prob(&self, a: char, b: char, c: char) -> f64 {
        let ratio = |num: Option<&f64>, den: Option<&f64>| match (num, den) {
            (Some(n), Some(d)) if *d > 0.0 => n / d,
            _ => 0.0,
        };
        let p3 = ratio(self.tri.get(&[a, b, c]), self.tri_ctx.get(&[a, b]));
        let p2 = ratio(self.bi.get(&[b, c]), self.bi_ctx.get(&b));
        let p1 = self.uni.get(&c).map_or(0.0, |n| n / self.total);
        0.6 * p3 + 0.3 * p2 + 0.09 * p1 + 0.01 / self.alphabet
    }

    /// Средний логарифм вероятности на тройку букв. Чем ближе к нулю,
    /// тем «естественнее» слово для языка.
    pub fn score(&self, word: &str) -> f64 {
        let chars = padded(word);
        let n = chars.len() - 2;
        let sum: f64 = chars.windows(3).map(|t| self.prob(t[0], t[1], t[2]).ln()).sum();
        sum / n as f64
    }
}

fn padded(word: &str) -> Vec<char> {
    let mut v = vec![START, START];
    v.extend(word.chars());
    v.push(END);
    v
}

/// Слова из списка частых, по порядку. Пропускает пустые строки и комментарии.
fn parse_core(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(normalize)
        .collect()
}

/// Всё, что известно об одном языке.
pub struct LanguageData {
    pub lang: Lang,
    pub model: CharModel,
    words: fst::Set<Vec<u8>>,
    core: HashMap<String, u32>,
}

impl LanguageData {
    /// Собирает данные из списка словоформ и списка частых слов.
    /// Медленно (секунды): так данные готовятся заранее, см. `examples/dictgen.rs`.
    pub fn build<'a>(lang: Lang, forms: impl Iterator<Item = &'a str>, core_text: &str) -> Self {
        let core_list = parse_core(core_text);
        let core_set: BTreeSet<&str> = core_list.iter().map(String::as_str).collect();
        let mut all: BTreeSet<String> = forms
            .map(normalize)
            .filter(|w| !w.is_empty() && w.chars().all(|c| lang.is_letter(c) || c == '-' || c == '\''))
            .collect();
        all.extend(core_list.iter().cloned());

        let model = CharModel::train(
            all.iter().map(|w| (w.as_str(), if core_set.contains(w.as_str()) { CORE_WEIGHT } else { 1.0 })),
        );
        let words = fst::Set::from_iter(all.iter()).expect("слова отсортированы");
        LanguageData { lang, model, words, core: Self::ranks(&core_list) }
    }

    /// Загружает заранее подготовленные данные.
    pub fn load(lang: Lang, fst_bytes: &[u8], trigrams: &str, core_text: &str) -> Self {
        LanguageData {
            lang,
            model: CharModel::from_text(trigrams),
            words: Self::set(fst_bytes.to_vec()),
            core: Self::ranks(&parse_core(core_text)),
        }
    }

    pub fn fst_bytes(&self) -> &[u8] {
        self.words.as_fst().as_bytes()
    }

    pub fn word_count(&self) -> usize {
        self.words.len()
    }

    fn set(bytes: Vec<u8>) -> fst::Set<Vec<u8>> {
        fst::Set::new(bytes).expect("повреждён файл словаря")
    }

    fn ranks(core: &[String]) -> HashMap<String, u32> {
        let mut ranks = HashMap::with_capacity(core.len());
        for (i, w) in core.iter().enumerate() {
            ranks.entry(w.clone()).or_insert(i as u32 + 1);
        }
        ranks
    }

    /// Есть ли в словаре слово, отличающееся от `word` на одну опечатку:
    /// лишняя, пропущенная или заменённая буква, две соседние буквы
    /// переставлены. Ожидает нормализованное слово.
    pub fn has_near(&self, word: &str) -> bool {
        let chars: Vec<char> = word.chars().collect();
        let alphabet: Vec<char> = match self.lang {
            Lang::En => ('a'..='z').collect(),
            Lang::Ru => ('а'..='я').collect(),
        };
        // Совпадения с одно-двухбуквенными словами слишком случайны.
        let known = |v: &[char]| v.len() >= 3 && self.rank(&v.iter().collect::<String>()).is_some();
        let mut v = chars.clone();
        for i in 0..chars.len() {
            // Пропущенная буква в слове — значит, в нашем варианте лишняя.
            v.remove(i);
            if known(&v) {
                return true;
            }
            v.insert(i, chars[i]);
            if i + 1 < chars.len() {
                v.swap(i, i + 1);
                if known(&v) {
                    return true;
                }
                v.swap(i, i + 1);
            }
            for &c in &alphabet {
                if c != chars[i] {
                    v[i] = c;
                    if known(&v) {
                        return true;
                    }
                }
            }
            v[i] = chars[i];
        }
        for i in 0..=chars.len() {
            for &c in &alphabet {
                v.insert(i, c);
                if known(&v) {
                    return true;
                }
                v.remove(i);
            }
        }
        false
    }

    /// Ранг слова: место в списке частых, `DICT_RANK` для прочих слов
    /// словаря, `None` — такого слова нет. Ожидает нормализованное слово.
    pub fn rank(&self, word: &str) -> Option<u32> {
        if let Some(r) = self.core.get(word) {
            return Some(*r);
        }
        if self.words.contains(word) {
            return Some(DICT_RANK);
        }
        // Английские сокращения, которых нет в списках: «we'd», «nobody's».
        if self.lang == Lang::En {
            if let Some((head, tail)) = word.split_once('\'') {
                if matches!(tail, "t" | "s" | "m" | "re" | "ve" | "ll" | "d") {
                    return self.rank(head);
                }
            }
        }
        None
    }
}
