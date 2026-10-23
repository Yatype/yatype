//! Проверка своей фразы: как бы её исправил определитель.
//!
//! cargo run --release --example try -- en "ghbdtn rfr ltkf"
//! cargo run --release --example try -- ru "руддщ цщкдв"

use detector::{Detector, Lang};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [layout, text] = args.as_slice() else {
        eprintln!("использование: try <en|ru> \"текст\"");
        std::process::exit(2);
    };
    let layout = if layout == "ru" { Lang::Ru } else { Lang::En };

    let words = Detector::builtin().analyze_text(text, layout);
    for w in &words {
        let mark = if w.switch { "→" } else { " " };
        println!("{mark} {:<16} {:<16} {:?}: {}", w.typed, w.converted, w.verdict, w.reason);
    }
    let fixed: Vec<&str> =
        words.iter().map(|w| if w.switch { w.converted.as_str() } else { w.typed.as_str() }).collect();
    println!("\nИтог: {}", fixed.join(" "));
}
