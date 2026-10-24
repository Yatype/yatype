//! Логика переключателя: что делать с каждым нажатием. Не зависит от
//! системы: всё системное — через `Platform`.

use std::time::Instant;

use detector::detector::apply_series_rule;
use detector::model::normalize;
use detector::{Detector, Lang, Verdict, WordResult};

use crate::double_tap::DoubleTap;
use crate::fixer::{self, Edit, WordView};
use crate::input::{Input, Pass};
use crate::platform::{Platform, Status};
use crate::settings::{Combo, Hotkey, Settings};
use crate::typing::{Key, Typing};

/// Последняя правка, которую можно отменить горячей клавишей.
struct LastFix {
    edit: Edit,
    /// Раскладка до правки.
    from: Lang,
    /// Слова, которые программа сочла неправильными. После отмены
    /// запоминаем их как исключения.
    fixed_words: Vec<String>,
}

pub struct Engine<P: Platform> {
    pub platform: P,
    detector: Detector,
    typing: Typing,
    lang: Option<Lang>,
    pub settings: Settings,
    /// Есть ли разрешение и стоит ли перехват.
    pub running: bool,
    /// Ждём, какое сочетание пользователь назначит горячей клавишей.
    pub recording: bool,
    dry_run: bool,
    /// Писать в лог набранные слова (только для отладки).
    verbose: bool,
    hotkey: DoubleTap,
    last_fix: Option<LastFix>,
}

impl<P: Platform> Engine<P> {
    pub fn new(mut platform: P, settings: Settings, dry_run: bool, verbose: bool) -> Self {
        let lang = platform.current_lang();
        Engine {
            platform,
            detector: Detector::builtin(),
            typing: Typing::default(),
            lang,
            settings,
            running: false,
            recording: false,
            dry_run,
            verbose: verbose || dry_run,
            hotkey: DoubleTap::default(),
            last_fix: None,
        }
    }

    fn log(&self, message: impl FnOnce() -> String) {
        if self.verbose {
            println!("{}", message());
        }
    }

    pub fn on_input(&mut self, input: Input) -> Pass {
        if self.recording {
            if let Input::Shortcut(combo) = input {
                self.record(combo);
                return Pass::Swallow;
            }
        }
        let lang = self.platform.current_lang();
        if lang != self.lang {
            // Раскладку переключили: набранное раньше читается уже по-другому.
            self.lang = lang;
            self.typing.reset();
            self.last_fix = None;
            self.refresh_status();
        }
        let Some(lang) = lang else { return Pass::Through };
        // Пауза: Yatype не вмешивается ни во что, даже горячая клавиша молчит
        // (в играх Control часто — «присесть»).
        if !self.settings.enabled {
            return Pass::Through;
        }

        match input {
            Input::Modifiers(held) => {
                if let Some((key, others)) = self.settings.hotkey.double_tap_state(held) {
                    if self.hotkey.modifiers(key, others, Instant::now()) {
                        self.hotkey_pressed(lang);
                    }
                }
                return Pass::Through;
            }
            Input::Shortcut(combo) if self.settings.hotkey == Hotkey::Combo && self.settings.combo == Some(combo) => {
                self.hotkey.other_key();
                self.hotkey_pressed(lang);
                return Pass::Swallow;
            }
            Input::Backspace if self.settings.backspace_undo && self.last_fix.is_some() => {
                self.hotkey.other_key();
                self.undo_last();
                return Pass::Swallow;
            }
            _ => {}
        }
        // Любая другая клавиша: отменять уже поздно, серия нажатий прервана.
        self.hotkey.other_key();
        self.last_fix = None;

        match input {
            Input::Key(key) => self.typing.key(key),
            Input::Backspace => self.typing.backspace(),
            Input::Reset | Input::Shortcut(_) | Input::Held => self.typing.reset(),
            Input::Space => {
                let pass = self.auto_fix(lang, false);
                self.typing.space();
                return pass;
            }
            Input::Enter => {
                let pass = self.auto_fix(lang, true);
                self.typing.reset();
                return pass;
            }
            Input::Modifiers(_) => unreachable!(),
        }
        Pass::Through
    }

    /// Слова фразы, включая текущее, с решением определителя.
    fn analyze(&self, lang: Lang) -> Vec<WordView> {
        let mut parts: Vec<(&[Key], usize)> = self.typing.words().iter().map(|w| (&w.keys[..], w.spaces)).collect();
        parts.push((self.typing.current(), 0));

        let mut results: Vec<WordResult> = parts
            .iter()
            .map(|(keys, _)| {
                let (typed, converted) = (self.platform.read(lang, keys), self.platform.read(lang.other(), keys));
                let mut r = self.detector.analyze_pair(&typed, &converted, lang);
                if self.settings.exception_words.contains(&normalize(&r.typed)) {
                    r.verdict = Verdict::Keep;
                    r.switch = false;
                    r.reason = "исключение пользователя";
                }
                r
            })
            .collect();
        apply_series_rule(&mut results);

        let w = results.last().expect("есть текущее слово");
        self.log(|| format!("[{lang:?}] «{}» → «{}»  {:?}: {}", w.typed, w.converted, w.verdict, w.reason));

        results
            .into_iter()
            .zip(parts)
            .map(|(r, (keys, spaces))| WordView {
                typed: r.typed,
                converted: r.converted,
                len: keys.len(),
                spaces,
                switch: r.switch,
            })
            .collect()
    }

    /// Слово закончено пробелом или Enter: исправить, если нужно.
    fn auto_fix(&mut self, lang: Lang, enter: bool) -> Pass {
        if !self.settings.enabled || self.typing.current().is_empty() {
            return Pass::Through;
        }
        // Проверяем до разбора слов: набранное в поле пароля не должно
        // попасть ни в определитель, ни в лог.
        if let Some(why) = self.platform.blocked(&self.settings) {
            self.typing.reset();
            self.log(|| format!("       пропускаю: {why}"));
            return Pass::Through;
        }
        let words = self.analyze(lang);
        // Одна буква без соседних слов («e» → «у») — самая частая ложная
        // тревога: так жмут клавиши в играх и программах. Ждём следующего
        // слова — если фраза продолжится, буква исправится вместе с ним.
        if words.iter().filter(|w| w.switch).all(|w| w.len == 1) {
            return Pass::Through;
        }
        let Some(mut edit) = fixer::plan(&words) else { return Pass::Through };
        if self.dry_run {
            self.log(|| format!("       исправил бы: «{}» → «{}»", edit.original, edit.insert));
            return Pass::Through;
        }

        let ending = if enter { "\n" } else { " " };
        self.platform.replace(edit.delete, &format!("{}{ending}", edit.insert));
        // После Enter сообщение уже ушло: отменять нечего.
        if !enter {
            edit.insert.push(' ');
            edit.original.push(' ');
        }
        self.log(|| format!("       ИСПРАВИЛ: «{}» → «{}»", edit.original.trim_end(), edit.insert.trim_end()));
        self.set_last(&format!("{} → {}", edit.original.trim_end(), edit.insert.trim_end()));

        let tail = &words[edit.first..];
        // Если в исправленном куске остались слова в старой раскладке, после
        // переключения их уже не прочитать правильно — начинаем заново.
        if tail.iter().all(|w| w.switch || w.typed == w.converted) {
            self.typing.drop_first(edit.first);
        } else {
            self.typing.reset();
        }
        let fixed_words = tail.iter().filter(|w| w.switch).map(|w| normalize(&w.typed)).collect();
        self.switch_to(lang.other());
        if !enter {
            self.last_fix = Some(LastFix { edit, from: lang, fixed_words });
        }
        Pass::Swallow
    }

    /// Горячая клавиша: отменить последнее исправление или перевести слово.
    fn hotkey_pressed(&mut self, lang: Lang) {
        if self.dry_run || self.undo_last() {
            return;
        }
        self.convert_last_word(lang);
    }

    /// Отменить последнюю правку. `false` — отменять нечего.
    fn undo_last(&mut self) -> bool {
        if self.dry_run {
            return false;
        }
        if let Some(fix) = self.last_fix.take() {
            let undo = fix.edit.undo();
            self.platform.replace(undo.delete, &undo.insert);
            for w in fix.fixed_words {
                if !self.settings.exception_words.contains(&w) {
                    self.settings.exception_words.push(w);
                }
            }
            self.platform.save_settings(&self.settings);
            self.typing.reset();
            self.switch_to(fix.from);
            self.log(|| format!("       ОТМЕНИЛ: вернул «{}»", undo.insert.trim_end()));
            self.set_last(&format!("отменено: {}", undo.insert.trim_end()));
            return true;
        }
        false
    }

    /// Перевести последнее слово в другую раскладку вручную.
    fn convert_last_word(&mut self, lang: Lang) {
        if let Some(why) = self.platform.blocked(&self.settings) {
            self.typing.reset();
            self.log(|| format!("       пропускаю: {why}"));
            return;
        }
        let (keys, spaces) = if !self.typing.current().is_empty() {
            (self.typing.current(), 0)
        } else if let Some(w) = self.typing.words().last() {
            (&w.keys[..], w.spaces)
        } else {
            return;
        };
        let view = WordView {
            typed: self.platform.read(lang, keys),
            converted: self.platform.read(lang.other(), keys),
            len: keys.len(),
            spaces,
            switch: true,
        };
        let edit = fixer::convert_word(&view);
        self.platform.replace(edit.delete, &edit.insert);
        self.log(|| format!("       ВРУЧНУЮ: «{}» → «{}»", view.typed, view.converted));
        self.set_last(&format!("{} → {}", view.typed, view.converted));
        self.typing.keep_last_word();
        self.switch_to(lang.other());
        // Повторное нажатие вернёт как было.
        self.last_fix = Some(LastFix { edit, from: lang, fixed_words: Vec::new() });
    }

    fn switch_to(&mut self, lang: Lang) {
        if !self.platform.select(lang) {
            eprintln!("Не удалось переключить раскладку на {lang:?}");
        }
        self.lang = Some(lang);
        self.refresh_status();
    }

    fn set_last(&mut self, text: &str) {
        self.platform.show_last(&format!("Последнее: {text}"));
    }

    /// Обновить значок: раскладка, пауза или нет доступа.
    pub fn refresh_status(&mut self) {
        let status = Status { lang: self.lang, running: self.running, enabled: self.settings.enabled };
        self.platform.show_status(status);
    }

    /// Пользователь нажал сочетание в режиме записи горячей клавиши.
    fn record(&mut self, combo: Combo) {
        if self.platform.is_escape(combo) {
            // Esc — передумал.
            self.recording = false;
        } else if combo.is_valid_hotkey(self.platform.is_function_key(combo.code)) {
            self.settings.combo = Some(combo);
            self.settings.hotkey = Hotkey::Combo;
            self.recording = false;
            self.settings_changed();
        } else {
            // Без Control/Option/Command сочетание мешало бы печатать: ждём другое.
            return;
        }
        self.platform.recording_finished();
    }

    /// Как показать сочетание пользователю, например «⌃⌥Z».
    pub fn describe(&self, combo: Combo) -> String {
        self.platform.describe(combo)
    }

    /// Пользователь поменял настройки: начать с чистого листа.
    pub fn settings_changed(&mut self) {
        self.platform.save_settings(&self.settings);
        self.typing.reset();
        self.last_fix = None;
        self.hotkey = DoubleTap::default();
        self.refresh_status();
    }
}
