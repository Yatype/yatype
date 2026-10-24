//! Автопереключатель раскладки для macOS.
//!
//! Слушает ввод, на пробеле или Enter проверяет фразу. Если слова набраны
//! не в той раскладке — стирает их, впечатывает правильно и переключает
//! раскладку. Горячая клавиша (по умолчанию двойной Control) отменяет
//! последнее исправление, а если отменять нечего — переводит последнее
//! слово вручную.
//!
//!   switcher             — работать
//!   switcher --verbose   — писать в лог решения по каждому слову (отладка)
//!   switcher --dry-run   — только писать в лог, что бы исправил
//!   switcher --check     — проверить раскладки без разрешений
//!   switcher --snapshot DIR — отрисовать окна в PNG (проверка интерфейса)
//!   switcher --settings  — сразу открыть окно настроек (проверка интерфейса)

mod app_list;
mod platform;
pub mod ui;

use std::cell::RefCell;
use std::rc::Rc;

use detector::{Detector, Lang};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSMenuItem, NSStatusBar, NSVariableStatusItemLength};

use engine::{Engine, Key};
use switcher::macos::{guard, keymap, settings_store};

use platform::MacPlatform;
use ui::Controller;

/// Ядро с macOS-платформой.
pub type App = Engine<MacPlatform>;

/// Проверка без разрешений: раскладки читаются и определитель работает
/// на кодах клавиш, как в настоящем вводе.
fn check() {
    let (Some(en), Some(ru)) = keymap::enabled_layouts() else {
        println!("Не нашёл включённых английской и русской раскладок.");
        return;
    };
    println!("Раскладки: {} и {}", en.id, ru.id);
    println!("Сейчас выбрана: {:?}", keymap::current_lang());
    println!(
        "Активное приложение: {}, исправлять можно: {}",
        guard::frontmost_app().unwrap_or_default(),
        guard::blocked(&engine::Settings::default()).map_or("да".to_string(), |why| format!("нет ({why})"))
    );

    // Клавиши g h b d t n — «ghbdtn» / «привет».
    let keys: Vec<Key> = [5u16, 4, 11, 2, 17, 45]
        .into_iter()
        .map(|code| Key { code, shift: false, caps: false })
        .collect();
    let (typed, other) = (en.read(&keys), ru.read(&keys));
    let w = Detector::builtin().analyze_pair(&typed, &other, Lang::En);
    println!("Клавиши g h b d t n: EN «{typed}», RU «{other}» → {:?}, исправить: {}", w.verdict, w.switch);
}

pub fn main() {
    let mtm = MainThreadMarker::new().expect("запускать нужно в главном потоке");
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    if flag("--check") {
        check();
        return;
    }

    let nsapp = NSApplication::sharedApplication(mtm);
    nsapp.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let (Some(en), Some(ru)) = keymap::enabled_layouts() else {
        ui::show_alert(
            mtm,
            "Нужны русская и английская раскладки",
            "Добавьте обе в «Системные настройки → Клавиатура → Источники ввода» и запустите программу снова.",
        );
        std::process::exit(1);
    };

    let status = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    let last = NSMenuItem::new(mtm);
    let platform = MacPlatform { en, ru, mtm, status: status.clone(), last: last.clone() };
    let app = Rc::new(RefCell::new(App::new(platform, settings_store::load(), flag("--dry-run"), flag("--verbose"))));

    let controller = Controller::new(mtm, app);
    controller.build_menu(&status, &last);
    if let Some(i) = args.iter().position(|a| a == "--snapshot") {
        ui::snapshot_windows(controller, args.get(i + 1).cloned().unwrap_or_else(|| ".".into()));
        nsapp.run();
        return;
    }
    if controller.start_if_permitted() {
        if flag("--verbose") || flag("--dry-run") {
            println!("Работаю.");
        }
    } else {
        controller.show_onboarding();
    }
    if flag("--settings") {
        let _: () = unsafe { objc2::msg_send![&*controller, showSettings: std::ptr::null::<objc2::runtime::AnyObject>()] };
    }
    ui::watch_permission(controller);
    nsapp.run();
}
