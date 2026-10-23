use std::sync::OnceLock;

use detector::{Detector, Lang, convert};

fn detector() -> &'static Detector {
    static D: OnceLock<Detector> = OnceLock::new();
    D.get_or_init(Detector::builtin)
}

/// Переводит фразу так, как это сделало бы приложение.
fn fix(text: &str, layout: Lang) -> String {
    detector()
        .analyze_text(text, layout)
        .iter()
        .map(|w| if w.switch { w.converted.clone() } else { w.typed.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn russian_typed_in_english_layout() {
    assert_eq!(fix("ghbdtn rfr ltkf&", Lang::En), "привет как дела?");
    assert_eq!(fix("Z ctujlyz pflth;ecm yf hf,jnt", Lang::En), "Я сегодня задержусь на работе");
}

#[test]
fn english_typed_in_russian_layout() {
    assert_eq!(fix("руддщ цщкдв", Lang::Ru), "hello world");
    assert_eq!(fix("Ш вщтэе лтщц", Lang::Ru), "I don't know");
}

#[test]
fn short_words_follow_their_neighbours() {
    // «z», «b», «f» по отдельности — английские буквы, но рядом с уверенно
    // неправильными словами это «я», «и», «а».
    assert_eq!(fix("z nt,z k.,k.", Lang::En), "я тебя люблю");
    assert_eq!(fix("vfvf b gfgf", Lang::En), "мама и папа");
}

#[test]
fn cases_from_real_typing() {
    // Из первого живого теста: мат и опечатки не исправлялись.
    assert_eq!(fix("gbplf [eq pfkegf", Lang::En), "пизда хуй залупа");
    assert_eq!(fix("vs blv", Lang::En), "мы идм");
    assert_eq!(fix("Gbdtn ctujlyz", Lang::En), "Пивет сегодня");
    assert_eq!(fix("ghbdtn? ,kznm", Lang::En), "привет, блять");
    assert_eq!(fix("pfx`v nt,t 'nj gbplt; dct ,skj yjhv", Lang::En), "зачём тебе это пиздеж все было норм");
}

#[test]
fn typos_are_fixed_in_context() {
    // «раскладка» с пропущенной буквой, «переключить» с переставленными.
    assert_eq!(fix("pf,sk gthtrk.xbnm hfcrkflre", Lang::En), "забыл переключить раскладку");
    assert_eq!(fix("pf,sk gtthrk.xbnm hfcrkflre", Lang::En), "забыл пеерключить раскладку");
}

#[test]
fn abbreviations_in_context() {
    assert_eq!(fix("e vtyz pfdnhf lh", Lang::En), "у меня завтра др");
    assert_eq!(fix("gbib vyt dj dr", Lang::En), "пиши мне во вк");
    assert_eq!(fix("crbym d nu", Lang::En), "скинь в тг");
}

#[test]
fn short_core_words_do_not_collide_across_layouts() {
    // Если короткое частое слово одного языка в другой раскладке оказалось
    // частым словом другого, определитель не сможет их различить.
    let core = |text: &str| -> Vec<String> {
        text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from).collect()
    };
    let ru = core(include_str!("../../../data/core/ru.txt"));
    let en = core(include_str!("../../../data/core/en.txt"));
    let mut clashes = Vec::new();
    for w in ru.iter().filter(|w| w.chars().count() <= 3) {
        let c = convert(w, Lang::Ru, Lang::En);
        if en.contains(&c) {
            clashes.push(format!("{w} ↔ {c}"));
        }
    }
    for w in en.iter().filter(|w| w.chars().count() <= 3) {
        let c = convert(w, Lang::En, Lang::Ru);
        if ru.contains(&c) {
            clashes.push(format!("{w} ↔ {c}"));
        }
    }
    assert!(clashes.is_empty(), "совпадения: {clashes:?}");
}

#[test]
fn correct_text_is_untouched() {
    for (text, layout) in [
        ("Привет, как дела?", Lang::Ru),
        ("Я тебя люблю", Lang::Ru),
        ("ну да и что", Lang::Ru),
        ("Hello, how are you?", Lang::En),
        ("I love you", Lang::En),
    ] {
        assert_eq!(fix(text, layout), text);
    }
}

#[test]
fn code_links_and_abbreviations_are_untouched() {
    for (text, layout) in [
        ("git commit -m fix", Lang::En),
        ("https://github.com/rust-lang/rust", Lang::En),
        ("user@example.com", Lang::En),
        ("password123", Lang::En),
        ("ООО РФ МГУ", Lang::Ru),
        ("USB HDMI API", Lang::En),
    ] {
        assert_eq!(fix(text, layout), text, "{text}");
    }
}

#[test]
fn a_lone_short_word_is_not_changed() {
    // Одного короткого слова мало: «lf» может оказаться чем угодно.
    // Приложение дождётся следующего слова.
    let w = detector().analyze_word("lf", Lang::En);
    assert!(!w.switch);
    assert_eq!(w.converted, "да");
}

#[test]
fn accuracy_on_test_phrases() {
    let d = detector();
    for (lang, text) in [
        (Lang::Ru, include_str!("../testdata/ru.txt")),
        (Lang::En, include_str!("../testdata/en.txt")),
    ] {
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let wrong: Vec<_> = d.analyze_text(line, lang).into_iter().filter(|w| w.switch).collect();
            assert!(wrong.is_empty(), "ложное срабатывание в «{line}»: {wrong:?}");

            let typed = convert(line, lang, lang.other());
            let missed: Vec<_> =
                d.analyze_text(&typed, lang.other()).into_iter().filter(|w| !w.switch).collect();
            assert!(missed.is_empty(), "не исправлено в «{typed}»: {missed:?}");
        }
    }
}
