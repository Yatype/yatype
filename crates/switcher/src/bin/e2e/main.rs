//! Сквозной тест: «печатает как человек» при запущенном переключателе.
//! Пока есть только для macOS.

#[cfg(target_os = "macos")]
mod macos;

fn main() {
    #[cfg(target_os = "macos")]
    macos::main();
    #[cfg(not(target_os = "macos"))]
    eprintln!("Сквозной тест пока есть только для macOS.");
}
