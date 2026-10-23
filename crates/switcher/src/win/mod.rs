//! Yatype для Windows: значок в трее и перехват клавиатуры.
//!
//!   yatype.exe            — работать
//!   yatype.exe --verbose  — писать в консоль решения по каждому слову (отладка)
//!   yatype.exe --dry-run  — только писать, что бы исправил

mod platform;
mod tray;

use std::cell::RefCell;

use engine::{Engine, Pass, Platform};
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MB_ICONWARNING, MB_OK, MSG, MessageBoxW, SetTimer, TranslateMessage,
};

use switcher::win32::{hook, keymap, settings_store, uia, wide};

use platform::WinPlatform;

/// Ядро с Windows-платформой.
pub type App = Engine<WinPlatform>;

thread_local! {
    static ENGINE: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Доступ к ядру из обработчиков. `None` — ядро занято (например, мы уже
/// внутри обработки нажатия) или ещё не создано.
pub fn with_engine<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    ENGINE.with(|e| e.try_borrow_mut().ok().and_then(|mut e| e.as_mut().map(f)))
}

fn warn(title: &str, text: &str) {
    unsafe { MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide(title).as_ptr(), MB_OK | MB_ICONWARNING) };
}

pub fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().any(|a| a == name);

    // Второй экземпляр не нужен: два перехвата мешали бы друг другу.
    unsafe {
        CreateMutexW(std::ptr::null(), 0, wide("Local\\Yatype").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
    }

    let (Some(en), Some(ru)) = keymap::installed_layouts() else {
        warn(
            "Yatype",
            "Нужны русская и английская раскладки.\n\nДобавьте обе в «Параметры → Время и язык → Язык и регион» и запустите программу снова.",
        );
        return;
    };

    // Следить за полями паролей (в том числе в браузерах) начинаем сразу.
    uia::start_watcher();

    let first_run = !settings_store::path().exists();
    let hwnd = tray::create_window();
    let platform = WinPlatform { en, ru, hwnd, last: String::new(), last_app: None, saved_at: None };
    let engine = App::new(platform, settings_store::load(), flag("--dry-run"), flag("--verbose"));
    ENGINE.with(|e| *e.borrow_mut() = Some(engine));

    let installed = hook::install(|input| with_engine(|e| e.on_input(input)).unwrap_or(Pass::Through));
    with_engine(|e| {
        e.running = installed.is_ok();
        if first_run {
            let settings = e.settings.clone();
            e.platform.save_settings(&settings);
        }
        e.refresh_status();
    });
    match installed {
        Ok(()) if first_run => tray::balloon(
            hwnd,
            "Yatype работает",
            "Значок EN/RU — в трее. Двойной Ctrl отменяет исправление или переводит слово вручную.",
        ),
        Ok(()) => {}
        Err(e) => warn("Yatype", e),
    }

    unsafe {
        SetTimer(hwnd, tray::TIMER_SETTINGS, 2000, None);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
