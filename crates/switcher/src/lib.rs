//! Системные слои Yatype. Логика — в крейте `engine`, здесь только то, как
//! каждая система отдаёт нажатия, печатает текст и переключает раскладку.

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(windows)]
pub mod win32;
