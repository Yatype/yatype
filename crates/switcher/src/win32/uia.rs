//! Поле пароля в фокусе — через UI Automation, системный интерфейс для
//! программ экранного доступа. Через него о полях паролей сообщают браузеры
//! (Chrome, Edge, Firefox) и современные программы, где стиль `ES_PASSWORD`
//! не виден.
//!
//! Спрашивать прямо из перехвата клавиатуры нельзя: запрос к другой
//! программе иногда занимает сотни миллисекунд, а Windows молча отключает
//! хук, который долго отвечает. Поэтому спрашивает отдельный поток, а
//! перехват мгновенно читает готовый ответ.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};

/// Как часто проверять фокус. Быстрее человек не успеет кликнуть в поле
/// пароля и закончить в нём слово.
const INTERVAL: Duration = Duration::from_millis(150);

static PASSWORD_FOCUSED: AtomicBool = AtomicBool::new(false);

/// Запустить фоновую проверку. Вызывать один раз при старте.
pub fn start_watcher() {
    thread::Builder::new()
        .name("yatype-uia".into())
        .spawn(|| unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                return;
            }
            let Ok(automation) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            else {
                return;
            };
            loop {
                let password = automation
                    .GetFocusedElement()
                    .and_then(|element| element.CurrentIsPassword())
                    .is_ok_and(|is| is.as_bool());
                PASSWORD_FOCUSED.store(password, Ordering::Relaxed);
                thread::sleep(INTERVAL);
            }
        })
        .ok();
}

/// В фокусе поле пароля (по последней проверке).
pub fn password_focused() -> bool {
    PASSWORD_FOCUSED.load(Ordering::Relaxed)
}
