//! Готовит встроенные данные определителя в `data/gen/`.
//!
//! Перед запуском разверните русский словарь:
//!   python3 tools/expand_hunspell.py data/hunspell/ru_RU > data/ru_forms.txt
//! Затем:
//!   cargo run --release --example dictgen

use std::fs;
use std::path::Path;

use detector::Lang;
use detector::model::LanguageData;

fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data = root.join("data");
    let out = data.join("gen");
    fs::create_dir_all(&out)?;

    for (lang, name, forms) in [(Lang::Ru, "ru", "ru_forms.txt"), (Lang::En, "en", "enable1.txt")] {
        let forms = fs::read_to_string(data.join(forms))?;
        let core = fs::read_to_string(data.join("core").join(format!("{name}.txt")))?;
        let d = LanguageData::build(lang, forms.lines(), &core);
        fs::write(out.join(format!("{name}.fst")), d.fst_bytes())?;
        fs::write(out.join(format!("{name}.tri")), d.model.to_text())?;
        println!("{name}: {} слов, словарь {} КБ", d.word_count(), d.fst_bytes().len() / 1024);
    }
    Ok(())
}
