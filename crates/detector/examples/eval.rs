//! Отчёт о точности определителя на фразах из `testdata/`.
//!
//! cargo run --release --example eval [-- -v]

use detector::model::LanguageData;
use detector::{Detector, Lang, WordResult, convert};

const RU: &str = include_str!("../testdata/ru.txt");
const EN: &str = include_str!("../testdata/en.txt");
const KEEP: &str = include_str!("../testdata/keep.txt");

#[derive(Default)]
struct Stats {
    words: usize,
    errors: usize,
    phrases: usize,
    phrases_ok: usize,
}

impl Stats {
    fn add(&mut self, results: &[WordResult], expect_switch: bool, verbose: bool) {
        self.phrases += 1;
        let mut ok = true;
        for w in results {
            self.words += 1;
            if w.switch != expect_switch {
                self.errors += 1;
                ok = false;
                if verbose {
                    println!("    {:>16} -> {:<16} {:?}: {}", w.typed, w.converted, w.verdict, w.reason);
                }
            }
        }
        if ok {
            self.phrases_ok += 1;
        }
    }

    fn print(&self, title: &str) {
        let pct = |a: usize, b: usize| 100.0 * a as f64 / b.max(1) as f64;
        println!(
            "{title}: слов верно {:.1}% ({} ошибок из {}), фраз целиком верно {:.1}% ({}/{})",
            pct(self.words - self.errors, self.words),
            self.errors,
            self.words,
            pct(self.phrases_ok, self.phrases),
            self.phrases_ok,
            self.phrases,
        );
    }
}

/// Опечатка в каждом слове от 5 букв: по очереди пропуск буквы и
/// перестановка двух соседних, в середине слова.
fn with_typos(line: &str) -> String {
    line.split(' ')
        .enumerate()
        .map(|(i, w)| {
            let mut c: Vec<char> = w.chars().collect();
            let letters = c.iter().filter(|ch| ch.is_alphabetic()).count();
            if letters >= 5 && c.iter().all(|ch| ch.is_alphabetic() || ",.?!".contains(*ch)) {
                let m = letters / 2;
                if i % 2 == 0 {
                    c.remove(m);
                } else {
                    c.swap(m - 1, m);
                }
            }
            c.into_iter().collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
    let verbose = std::env::args().any(|a| a == "-v");
    let d = Detector::builtin();

    let corpora = [(Lang::Ru, RU), (Lang::En, EN)];

    // Набрано в правильной раскладке: ничего не должно переводиться.
    for (lang, text) in corpora {
        let mut s = Stats::default();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            s.add(&d.analyze_text(line, lang), false, verbose);
        }
        s.print(&format!("{lang:?} в своей раскладке  "));
    }

    // Набрано в чужой раскладке: всё должно переводиться обратно.
    for (lang, text) in corpora {
        let typed_in = lang.other();
        let mut s = Stats::default();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let typed = convert(line, lang, typed_in);
            s.add(&d.analyze_text(&typed, typed_in), true, verbose);
        }
        s.print(&format!("{lang:?} в чужой раскладке  "));
    }

    // Те же фразы с опечаткой в каждом длинном слове.
    for (lang, text) in corpora {
        let (mut own, mut wrong) = (Stats::default(), Stats::default());
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let typo = with_typos(line);
            own.add(&d.analyze_text(&typo, lang), false, verbose);
            let typed = convert(&typo, lang, lang.other());
            wrong.add(&d.analyze_text(&typed, lang.other()), true, verbose);
        }
        own.print(&format!("{lang:?} с опечатками, своя "));
        wrong.print(&format!("{lang:?} с опечатками, чужая"));
    }

    // Особые случаи, которые трогать нельзя.
    let mut s = Stats::default();
    for line in KEEP.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let (lang, text) = line.split_once('\t').expect("раскладка<TAB>текст");
        let lang = if lang == "ru" { Lang::Ru } else { Lang::En };
        s.add(&d.analyze_text(text, lang), false, verbose);
    }
    s.print("Особые случаи (не трогать)");

    // Незнакомые слова: прячем каждую 10-ю словоформу из словаря и из
    // обучения модели, потом проверяем на спрятанных. Решает только модель.
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let read = |name: &str| std::fs::read_to_string(data.join(name)).expect("нет файла в data/");
    let (ru_forms, en_forms) = (read("ru_forms.txt"), read("enable1.txt"));
    let (ru_core, en_core) = (read("core/ru.txt"), read("core/en.txt"));
    fn train(forms: &str) -> impl Iterator<Item = &str> {
        forms.lines().enumerate().filter(|(i, _)| i % 10 != 0).map(|(_, w)| w)
    }
    let blind = Detector::new(
        LanguageData::build(Lang::En, train(&en_forms), &en_core),
        LanguageData::build(Lang::Ru, train(&ru_forms), &ru_core),
    );
    println!("\nНезнакомые слова (без словаря, только модель):");
    for (lang, forms) in [(Lang::Ru, &ru_forms), (Lang::En, &en_forms)] {
        let words: Vec<&str> = forms
            .lines()
            .step_by(10)
            .filter(|w| w.chars().count() >= 4 && w.chars().all(|c| lang.is_letter(c)))
            .step_by(if lang == Lang::Ru { 7 } else { 1 })
            .collect();
        let kept = words.iter().filter(|w| !blind.analyze_word(w, lang).switch).count();
        let fixed = words
            .iter()
            .filter(|w| blind.analyze_word(&convert(w, lang, lang.other()), lang.other()).switch)
            .count();
        let pct = |a: usize| 100.0 * a as f64 / words.len() as f64;
        println!(
            "  {lang:?}: {} слов; в своей раскладке не тронуто {:.1}%, в чужой исправлено {:.1}%",
            words.len(),
            pct(kept),
            pct(fixed)
        );
        if verbose {
            let bad: Vec<_> = words.iter().filter(|w| blind.analyze_word(w, lang).switch).take(15).collect();
            println!("    ложные срабатывания, примеры: {bad:?}");
        }
    }
}
