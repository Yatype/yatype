//! Определитель неправильной раскладки для пары русский ↔ английский.
//!
//! ```
//! let d = detector::Detector::builtin();
//! let w = d.analyze_word("ghbdtn", detector::Lang::En);
//! assert!(w.switch);
//! assert_eq!(w.converted, "привет");
//! ```

pub mod detector;
pub mod layout;
pub mod model;

pub use detector::{Config, Detector, Verdict, WordResult};
pub use layout::{Lang, convert};

use model::LanguageData;

// Данные готовит `examples/dictgen.rs`. Источники: русский — словарь
// Hunspell А. Лебедева (см. data/LICENSES.txt), английский — список ENABLE
// (общественное достояние), частые и современные слова — собственные списки
// проекта в data/core/.
const RU_FST: &[u8] = include_bytes!("../../../data/gen/ru.fst");
const RU_TRI: &str = include_str!("../../../data/gen/ru.tri");
const RU_CORE: &str = include_str!("../../../data/core/ru.txt");
const EN_FST: &[u8] = include_bytes!("../../../data/gen/en.fst");
const EN_TRI: &str = include_str!("../../../data/gen/en.tri");
const EN_CORE: &str = include_str!("../../../data/core/en.txt");

impl Detector {
    /// Определитель на встроенных словарях.
    pub fn builtin() -> Self {
        Detector::new(
            LanguageData::load(Lang::En, EN_FST, EN_TRI, EN_CORE),
            LanguageData::load(Lang::Ru, RU_FST, RU_TRI, RU_CORE),
        )
    }
}
