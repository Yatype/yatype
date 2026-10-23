//! Yatype: исправляет текст, набранный не в той раскладке.

// На Windows без чёрного окна консоли (кроме отладочной сборки).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod win;

fn main() {
    #[cfg(target_os = "macos")]
    mac::main();
    #[cfg(windows)]
    win::main();
    #[cfg(not(any(target_os = "macos", windows)))]
    eprintln!("Эта система пока не поддерживается.");
}
