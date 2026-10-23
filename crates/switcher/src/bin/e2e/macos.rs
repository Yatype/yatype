//! Сквозной тест: «печатает как человек» в тестовое окно при запущенном
//! переключателе и сверяет, что получилось.
//!
//! Нажатия отправляются настоящими кодами клавиш, как от физической
//! клавиатуры, с человеческой скоростью. Переключатель запускается отдельным
//! процессом, как у пользователя. Во время теста нельзя трогать клавиатуру
//! и мышь: нажатия идут в активное окно.
//!
//!   cargo build --release && ./target/release/e2e [--quick]

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread::sleep;
use std::time::Duration;

use detector::{Lang, convert};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSResponder, NSSecureTextField, NSTextView,
    NSWindow, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use switcher::macos::ffi::*;
use switcher::macos::keymap::{self, KeyLayout};
use engine::Key;

const RU: &str = include_str!("../../../../detector/testdata/ru.txt");
const EN: &str = include_str!("../../../../detector/testdata/en.txt");

// ---------- Тестовое окно (главный поток) ----------

/// Текст в поле и в поле пароля, копируется из главного потока по таймеру.
static TEXT: Mutex<String> = Mutex::new(String::new());
static SECRET: Mutex<String> = Mutex::new(String::new());
/// Просьба главному потоку: 1 — очистить и встать в поле текста,
/// 2 — очистить и встать в поле пароля, 3 — вывести окно вперёд.
static REQUEST: AtomicU8 = AtomicU8::new(0);
/// Наше окно сейчас активно.
static FRONT: AtomicBool = AtomicBool::new(false);
/// Раскладки трогаем только из главного потока (так требует macOS):
/// просьба выбрать раскладку и текущая раскладка. 0 — нет, 1 — EN, 2 — RU.
static SELECT_LANG: AtomicU8 = AtomicU8::new(0);
static CURRENT_LANG: AtomicU8 = AtomicU8::new(0);

fn lang_code(lang: Option<Lang>) -> u8 {
    match lang {
        Some(Lang::En) => 1,
        Some(Lang::Ru) => 2,
        None => 0,
    }
}

struct Ui {
    en: KeyLayout,
    ru: KeyLayout,
    app: Retained<NSApplication>,
    window: Retained<NSWindow>,
    text: Retained<NSTextView>,
    secret: Retained<NSSecureTextField>,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

#[allow(deprecated)] // activateIgnoringOtherApps: окно теста должно стать активным
fn bring_to_front(ui: &Ui) {
    ui.window.makeKeyAndOrderFront(None);
    ui.app.activateIgnoringOtherApps(true);
}

extern "C" fn tick(_timer: *mut c_void, _info: *mut c_void) {
    UI.with(|ui| {
        let ui = ui.borrow();
        let Some(ui) = ui.as_ref() else { return };
        match REQUEST.swap(0, Ordering::SeqCst) {
            1 => {
                ui.text.setString(&NSString::from_str(""));
                let r: &NSResponder = &ui.text;
                ui.window.makeFirstResponder(Some(r));
            }
            2 => {
                ui.secret.setStringValue(&NSString::from_str(""));
                let r: &NSResponder = &ui.secret;
                ui.window.makeFirstResponder(Some(r));
            }
            3 => bring_to_front(ui),
            _ => {}
        }
        match SELECT_LANG.swap(0, Ordering::SeqCst) {
            1 => {
                ui.en.select();
            }
            2 => {
                ui.ru.select();
            }
            _ => {}
        }
        CURRENT_LANG.store(lang_code(keymap::current_lang()), Ordering::SeqCst);
        *TEXT.lock().unwrap() = ui.text.string().to_string();
        *SECRET.lock().unwrap() = ui.secret.stringValue().to_string();
        let front = NSWorkspace::sharedWorkspace().frontmostApplication().map(|a| a.processIdentifier());
        FRONT.store(front == Some(std::process::id() as i32), Ordering::SeqCst);
    });
}

fn build_window(mtm: MainThreadMarker, en: KeyLayout, ru: KeyLayout) -> Ui {
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let frame = NSRect::new(NSPoint::new(300.0, 300.0), NSSize::new(640.0, 260.0));
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            frame,
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str("Сквозной тест переключателя — не трогайте клавиатуру"));
    let content = window.contentView().expect("есть содержимое окна");

    let text = NSTextView::initWithFrame(mtm.alloc(), NSRect::new(NSPoint::new(10.0, 60.0), NSSize::new(620.0, 190.0)));
    text.setRichText(false);
    // Никаких автозамен macOS: проверяем только переключатель.
    text.setEnabledTextCheckingTypes(0);
    text.setAutomaticQuoteSubstitutionEnabled(false);
    content.addSubview(&text);

    let secret =
        NSSecureTextField::initWithFrame(mtm.alloc(), NSRect::new(NSPoint::new(10.0, 15.0), NSSize::new(620.0, 30.0)));
    content.addSubview(&secret);

    let ui = Ui { en, ru, app, window, text, secret };
    bring_to_front(&ui);
    ui
}

// ---------- «Пользователь» (отдельный поток) ----------

const SHIFT_KEY: u16 = 56;
const CONTROL_KEY: u16 = 59;
const RETURN: u16 = 36;
const BACKSPACE: u16 = 51;
const LEFT_ARROW: u16 = 123;

fn post(code: u16, down: bool, flags: u64, flags_changed: bool) {
    unsafe {
        let e = CGEventCreateKeyboardEvent(std::ptr::null(), code, down);
        if flags_changed {
            CGEventSetType(e, kCGEventFlagsChanged);
        }
        CGEventSetFlags(e, flags);
        CGEventPost(kCGHIDEventTap, e);
        CFRelease(e);
    }
}

fn press(key: Key, delay: u64) {
    let flags = if key.shift { kCGEventFlagMaskShift } else { 0 };
    if key.shift {
        post(SHIFT_KEY, true, flags, true);
    }
    post(key.code, true, flags, false);
    post(key.code, false, flags, false);
    if key.shift {
        post(SHIFT_KEY, false, 0, true);
    }
    sleep(Duration::from_millis(delay));
}

fn tap(code: u16, delay: u64) {
    press(Key { code, shift: false, caps: false }, delay);
}

fn double_control() {
    for _ in 0..2 {
        post(CONTROL_KEY, true, kCGEventFlagMaskControl, true);
        sleep(Duration::from_millis(50));
        post(CONTROL_KEY, false, 0, true);
        sleep(Duration::from_millis(100));
    }
    sleep(Duration::from_millis(200));
}

#[derive(Clone)]
enum Step {
    /// Нажать клавиши, которые дают этот текст в исходной раскладке
    /// сценария (физические клавиши — дальше раскладка может смениться).
    Keys(String, u64),
    Backspace,
    DoubleControl,
}

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Text,
    Password,
}

struct Scenario {
    name: String,
    start: Lang,
    field: Field,
    steps: Vec<Step>,
    expect: String,
    /// Какая раскладка должна стоять в конце.
    expect_lang: Lang,
}

fn keys(s: &str) -> Step {
    Step::Keys(s.to_string(), 30)
}

fn scenario(name: &str, start: Lang, steps: Vec<Step>, expect: &str, expect_lang: Lang) -> Scenario {
    Scenario { name: name.into(), start, field: Field::Text, steps, expect: expect.into(), expect_lang }
}

fn scenarios(quick: bool) -> Vec<Scenario> {
    use Lang::{En, Ru};
    let mut list = Vec::new();

    let phrases = |text: &'static str| text.lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>();
    let take = |v: Vec<&'static str>, step: usize| v.into_iter().step_by(step).collect::<Vec<_>>();
    let (ru, en) = (phrases(RU), phrases(EN));
    let (wrong_step, right_step) = if quick { (6, 12) } else { (1, 2) };

    // Забыл переключить раскладку: всё должно исправиться.
    for p in take(ru.clone(), wrong_step) {
        let typed = convert(p, Ru, En);
        list.push(scenario(&format!("RU в EN: {p}"), En, vec![keys(&format!("{typed} "))], &format!("{p} "), Ru));
    }
    for p in take(en.clone(), wrong_step) {
        let typed = convert(p, En, Ru);
        list.push(scenario(&format!("EN в RU: {p}"), Ru, vec![keys(&format!("{typed} "))], &format!("{p} "), En));
    }
    // Правильная раскладка: ничего не трогать.
    for p in take(ru, right_step) {
        list.push(scenario(&format!("RU верно: {p}"), Ru, vec![keys(&format!("{p} "))], &format!("{p} "), Ru));
    }
    for p in take(en, right_step) {
        list.push(scenario(&format!("EN верно: {p}"), En, vec![keys(&format!("{p} "))], &format!("{p} "), En));
    }

    // Особые случаи.
    list.push(scenario("Enter отправляет исправленное", En, vec![keys("ghbdtn\n")], "привет\n", Ru));
    list.push(scenario("Enter, английский в RU", Ru, vec![keys("руддщ\n")], "hello\n", En));
    list.push(scenario(
        "Исправление через Backspace",
        En,
        vec![keys("ghbdtt"), Step::Backspace, keys("n ")],
        "привет ",
        Ru,
    ));
    list.push(scenario("Заглавные и знаки", En, vec![keys("Ghbdtn? rfr ltkf& ")], "Привет, как дела? ", Ru));
    list.push(scenario("Английское слово перед русским", En, vec![keys("ok ghbdtn ")], "ok привет ", Ru));
    list.push(scenario("Код", En, vec![keys("git commit -m fix ")], "git commit -m fix ", En));
    list.push(scenario("Ссылка", En, vec![keys("https://github.com/rust ")], "https://github.com/rust ", En));
    list.push(scenario("Почта", En, vec![keys("user@example.com ")], "user@example.com ", En));
    list.push(scenario(
        "Опечатки",
        En,
        vec![keys("pf,sk gtthrk.xbnm hfcrkflre ")],
        "забыл пеерключить раскладку ",
        Ru,
    ));
    list.push(scenario("Мат", En, vec![keys("gbplf [eq pfkegf ")], "пизда хуй залупа ", Ru));
    list.push(scenario("Аббревиатуры", En, vec![keys("crbym d nu ")], "скинь в тг ", Ru));
    list.push(scenario(
        "Быстрая печать (8 мс)",
        En,
        vec![Step::Keys("ghbdtn rfr ltkf& z pf,sk gthtrk.xbnm hfcrkflre ".into(), 8)],
        "привет как дела? я забыл переключить раскладку ",
        Ru,
    ));
    list.push(scenario(
        "Гонка: печать без пауз (3 мс), несколько исправлений",
        Ru,
        vec![Step::Keys(convert("What's up bro? I need to renew my insurance ", En, Ru), 3)],
        "What's up bro? I need to renew my insurance ",
        En,
    ));
    list.push(scenario(
        "Ручной перевод текущего слова",
        Ru,
        // Клавиши задаются по исходной раскладке: «цщкдв» — это w o r l d.
        vec![keys("руддщ"), Step::DoubleControl, keys(" цщкдв ")],
        "hello world ",
        En,
    ));
    list.push(scenario(
        "Ручной перевод короткого слова",
        En,
        vec![keys("lf "), Step::DoubleControl],
        "да ",
        Ru,
    ));
    // Отмены в конце: отменённое слово становится исключением.
    list.push(scenario(
        "Отмена через Backspace сразу после исправления",
        En,
        vec![keys("pfdnhf "), Step::Backspace],
        "pfdnhf ",
        En,
    ));
    list.push(scenario(
        "Отмена исправления",
        En,
        vec![keys("ckjdj "), Step::DoubleControl],
        "ckjdj ",
        En,
    ));
    list.push(scenario(
        "После отмены слово не трогаем",
        En,
        vec![keys("ckjdj ")],
        "ckjdj ",
        En,
    ));
    list.push(Scenario {
        name: "Поле пароля".into(),
        start: En,
        field: Field::Password,
        steps: vec![keys("ghbdtn ")],
        expect: "ghbdtn ".into(),
        expect_lang: En,
    });
    list
}

fn wait_front() -> bool {
    for _ in 0..3 {
        if FRONT.load(Ordering::SeqCst) {
            return true;
        }
        REQUEST.store(3, Ordering::SeqCst);
        sleep(Duration::from_millis(700));
    }
    FRONT.load(Ordering::SeqCst)
}

fn run(scenarios: Vec<Scenario>, mut switcher: Child, en: HashMap<char, Key>, ru: HashMap<char, Key>) -> i32 {
    let keys_of = |l: Lang| if l == Lang::En { &en } else { &ru };
    sleep(Duration::from_millis(1500)); // переключатель загружается

    let (mut passed, mut failed) = (0, Vec::new());
    for s in &scenarios {
        if !wait_front() {
            eprintln!("Тестовое окно перестало быть активным — останавливаю тест.");
            failed.push(format!("{}: окно не активно, тест прерван", s.name));
            break;
        }
        REQUEST.store(if s.field == Field::Text { 1 } else { 2 }, Ordering::SeqCst);
        sleep(Duration::from_millis(150));
        SELECT_LANG.store(lang_code(Some(s.start)), Ordering::SeqCst);
        sleep(Duration::from_millis(250));
        tap(LEFT_ARROW, 30); // переключатель забывает прошлый сценарий

        let typing = keys_of(s.start);
        for step in &s.steps {
            match step {
                Step::Keys(text, delay) => {
                    for c in text.chars() {
                        if c == '\n' {
                            tap(RETURN, *delay);
                        } else {
                            let Some(&key) = typing.get(&c) else {
                                eprintln!("нет клавиши для «{c}»");
                                continue;
                            };
                            press(key, *delay);
                        }
                    }
                }
                Step::Backspace => tap(BACKSPACE, 30),
                Step::DoubleControl => double_control(),
            }
        }
        sleep(Duration::from_millis(400));

        let got = if s.field == Field::Text { TEXT.lock().unwrap().clone() } else { SECRET.lock().unwrap().clone() };
        let lang = match CURRENT_LANG.load(Ordering::SeqCst) {
            1 => Some(Lang::En),
            2 => Some(Lang::Ru),
            _ => None,
        };
        if got == s.expect && lang == Some(s.expect_lang) {
            passed += 1;
            println!("  ок   {}", s.name);
        } else {
            println!("  ОШИБКА {}\n         ждал «{}» ({:?}), получил «{}» ({:?})", s.name, s.expect.escape_debug(), s.expect_lang, got.escape_debug(), lang);
            failed.push(s.name.clone());
        }
    }

    let _ = switcher.kill();
    let _ = Command::new("defaults").args(["delete", "switcher"]).stderr(Stdio::null()).status();
    println!("\nИтого: {passed} из {} сценариев прошли.", scenarios.len());
    if !failed.is_empty() {
        println!("Не прошли:");
        for f in &failed {
            println!("  - {f}");
        }
    }
    if failed.is_empty() { 0 } else { 1 }
}

pub fn main() {
    let mtm = MainThreadMarker::new().expect("главный поток");
    if unsafe { AXIsProcessTrusted() } == 0 {
        eprintln!("Нет разрешения «Универсальный доступ» для программы, из которой запущен тест.");
        std::process::exit(2);
    }
    let quick = std::env::args().any(|a| a == "--quick");

    let exe = std::env::current_exe().expect("путь к тесту");
    let dir = exe.parent().expect("папка сборки");
    let log = std::fs::File::create(dir.join("e2e-switcher.log")).expect("лог переключателя");
    // Чистые настройки: исключения от прошлых прогонов не должны мешать.
    // Отмену через Backspace включаем: её проверяет отдельный сценарий.
    let _ = Command::new("defaults").args(["delete", "switcher"]).stderr(Stdio::null()).status();
    let _ = Command::new("defaults").args(["write", "switcher", "backspaceUndo", "-bool", "true"]).status();
    let switcher = Command::new(dir.join("switcher"))
        .arg("--verbose")
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("не запустился switcher — сначала cargo build --release");

    let (Some(en), Some(ru)) = keymap::enabled_layouts() else {
        eprintln!("Нужны включённые английская и русская раскладки");
        std::process::exit(2);
    };
    let (en_keys, ru_keys) = (en.keys_for_chars(), ru.keys_for_chars());
    let ui = build_window(mtm, en, ru);
    UI.with(|cell| *cell.borrow_mut() = Some(ui));
    unsafe {
        let timer = CFRunLoopTimerCreate(std::ptr::null(), CFAbsoluteTimeGetCurrent(), 0.02, 0, 0, tick, std::ptr::null());
        CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
    }

    let list = scenarios(quick);
    println!("Сценариев: {}. Не трогайте клавиатуру и мышь до конца теста.\n", list.len());
    std::thread::spawn(move || {
        let code = run(list, switcher, en_keys, ru_keys);
        std::process::exit(code);
    });
    NSApplication::sharedApplication(mtm).run();
}
