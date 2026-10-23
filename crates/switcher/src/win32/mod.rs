pub mod autostart;
pub mod guard;
pub mod hook;
pub mod inject;
pub mod keymap;
pub mod keys;
pub mod settings_store;
pub mod uia;

/// Строка для Windows API: UTF-16 с нулём в конце.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Строка из буфера UTF-16 до первого нуля.
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
