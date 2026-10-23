//! Значок в трее: «EN»/«RU», меню по клику, всплывающие подсказки.

use detector::Lang;
use engine::{Hotkey, Platform, Status};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateBitmap, CreateCompatibleDC,
    CreateDIBSection, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, FW_BOLD, FillRect, GetDC, OUT_DEFAULT_PRECIS,
    ReleaseDC, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    ShellExecuteW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    GetCursorPos, HICON, HMENU, ICONINFO, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SW_SHOWNORMAL, SetForegroundWindow, TPM_BOTTOMALIGN,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
    WM_TIMER, WNDCLASSW,
};

use switcher::win32::{autostart, settings_store, wide};

use super::with_engine;

/// Сообщение от значка в трее (клик).
pub const WM_TRAY: u32 = WM_APP + 1;
/// Запись горячей клавиши закончилась.
pub const WM_RECORDED: u32 = WM_APP + 2;
/// Таймер проверки файла настроек (правки из Блокнота).
pub const TIMER_SETTINGS: usize = 1;

const CLASS: &str = "YatypeTray";

// Пункты меню.
const CMD_ENABLED: usize = 1;
const CMD_EXCLUDE_APP: usize = 2;
const CMD_HOTKEY: usize = 10; // 10…14 — варианты горячей клавиши
const CMD_BACKSPACE_UNDO: usize = 20;
const CMD_AUTOSTART: usize = 21;
const CMD_LISTS: usize = 30;
const CMD_LICENSES: usize = 31;
const CMD_QUIT: usize = 99;

/// Скрытое окно, которому приходят сообщения значка и меню.
pub fn create_window() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = wide(CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            0,
            class.as_ptr(),
            wide("Yatype").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    }
}

fn copy_into(dst: &mut [u16], s: &str) {
    let src: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..src.len()].copy_from_slice(&src);
    dst[src.len()] = 0;
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data
}

/// Квадратный значок с надписью — как «EN»/«RU» у системной раскладки.
fn draw_icon(text: &str, background: COLORREF) -> HICON {
    const SIZE: i32 = 32;
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let dc = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: SIZE,
            biHeight: -SIZE,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let old = SelectObject(dc, color);

        let mut rect = RECT { left: 0, top: 0, right: SIZE, bottom: SIZE };
        let brush = CreateSolidBrush(background);
        FillRect(dc, &rect, brush);
        DeleteObject(brush);

        let font = CreateFontW(
            -19,
            0,
            0,
            0,
            FW_BOLD as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            ANTIALIASED_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        );
        let old_font = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0x00FF_FFFF);
        let label = wide(text);
        DrawTextW(dc, label.as_ptr(), -1, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        SelectObject(dc, old_font);
        DeleteObject(font);

        // GDI не заполняет прозрачность: делаем все точки непрозрачными.
        if !bits.is_null() {
            let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (SIZE * SIZE) as usize);
            for p in pixels {
                *p |= 0xFF00_0000;
            }
        }
        SelectObject(dc, old);

        let mask = CreateBitmap(SIZE, SIZE, 1, 1, std::ptr::null());
        let icon_info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&icon_info);
        DeleteObject(mask);
        DeleteObject(color);
        DeleteDC(dc);
        ReleaseDC(std::ptr::null_mut(), screen);
        icon
    }
}

/// Показать (или обновить) значок по состоянию программы.
pub fn set_status(hwnd: HWND, status: Status) {
    let lang = match status.lang {
        Some(Lang::En) => "EN",
        Some(Lang::Ru) => "RU",
        None => "··",
    };
    // COLORREF — 0x00BBGGRR.
    let (background, tip) = if !status.running {
        (0x0030_30D0, "Yatype — перехват клавиатуры не работает".to_string())
    } else if !status.enabled {
        (0x0080_8080, format!("Yatype — {lang}, пауза"))
    } else {
        (0x00E6_6C2B, format!("Yatype — {lang}"))
    };
    let icon = draw_icon(lang, background);
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = WM_TRAY;
    data.hIcon = icon;
    copy_into(&mut data.szTip, &tip);
    unsafe {
        // Если значка ещё нет (или Проводник перезапускался) — добавить.
        if Shell_NotifyIconW(NIM_MODIFY, &data) == 0 {
            Shell_NotifyIconW(NIM_ADD, &data);
        }
        DestroyIcon(icon);
    }
}

/// Всплывающая подсказка у значка.
pub fn balloon(hwnd: HWND, title: &str, text: &str) {
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_INFO;
    data.dwInfoFlags = NIIF_INFO;
    copy_into(&mut data.szInfoTitle, title);
    copy_into(&mut data.szInfo, text);
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
}

fn hotkey_title(h: Hotkey) -> &'static str {
    match h {
        Hotkey::Control => "Двойной Ctrl",
        Hotkey::Shift => "Двойной Shift",
        Hotkey::Option => "Двойной Alt",
        Hotkey::Command => "Двойной Win",
        Hotkey::Combo => "Своё сочетание…",
    }
}

fn add_item(menu: HMENU, id: usize, title: &str, checked: bool, enabled: bool) {
    let mut flags = MF_STRING;
    if checked {
        flags |= MF_CHECKED;
    }
    if !enabled {
        flags |= MF_GRAYED;
    }
    unsafe { AppendMenuW(menu, flags, id, wide(title).as_ptr()) };
}

fn separator(menu: HMENU) {
    unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null()) };
}

/// Меню по клику на значок.
fn show_menu(hwnd: HWND) {
    let Some((settings, last, last_app, combo, recording)) = with_engine(|e| {
        (
            e.settings.clone(),
            e.platform.last.clone(),
            e.platform.last_app.clone(),
            e.settings.combo.map(|c| e.describe(c)),
            e.recording,
        )
    }) else {
        return;
    };
    unsafe {
        let menu = CreatePopupMenu();
        if !last.is_empty() {
            add_item(menu, 0, &last, false, false);
            separator(menu);
        }
        add_item(menu, CMD_ENABLED, "Исправлять автоматически", settings.enabled, true);
        if let Some((file, name)) = &last_app {
            let excluded = settings.excluded_apps.contains(file);
            add_item(menu, CMD_EXCLUDE_APP, &format!("Не исправлять в «{name}»"), excluded, true);
        }
        separator(menu);

        let hotkeys = CreatePopupMenu();
        for (i, h) in Hotkey::ALL.iter().enumerate() {
            let title = match (h, combo.as_deref()) {
                (Hotkey::Combo, _) if recording => "Своё сочетание: нажмите… (Esc — отмена)".to_string(),
                (Hotkey::Combo, Some(c)) => format!("Своё сочетание: {c} (записать заново…)"),
                _ => hotkey_title(*h).to_string(),
            };
            add_item(hotkeys, CMD_HOTKEY + i, &title, settings.hotkey == *h, true);
        }
        AppendMenuW(menu, MF_POPUP, hotkeys as usize, wide("Отмена и ручной перевод").as_ptr());
        add_item(
            menu,
            CMD_BACKSPACE_UNDO,
            "Backspace сразу после исправления отменяет его",
            settings.backspace_undo,
            true,
        );
        add_item(menu, CMD_AUTOSTART, "Запускать при входе в Windows", autostart::enabled(), true);
        separator(menu);
        add_item(menu, CMD_LISTS, "Исключения и настройки (файл)…", false, true);
        add_item(menu, CMD_LICENSES, "Лицензии словарей", false, true);
        separator(menu);
        add_item(menu, CMD_QUIT, "Выход", false, true);

        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        // Без этого меню не закрывается по клику мимо.
        SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            point.x,
            point.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        if command > 0 {
            run_command(hwnd, command as usize, last_app);
        }
    }
}

fn open(file: &str, args: Option<&str>) {
    let args = args.map(wide);
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            wide("open").as_ptr(),
            wide(file).as_ptr(),
            args.as_ref().map_or(std::ptr::null(), |a| a.as_ptr()),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
}

fn run_command(hwnd: HWND, command: usize, last_app: Option<(String, String)>) {
    match command {
        CMD_ENABLED => {
            with_engine(|e| {
                e.settings.enabled = !e.settings.enabled;
                e.settings_changed();
            });
        }
        CMD_EXCLUDE_APP => {
            if let Some((file, _)) = last_app {
                with_engine(|e| {
                    let list = &mut e.settings.excluded_apps;
                    if let Some(i) = list.iter().position(|a| *a == file) {
                        list.remove(i);
                    } else {
                        list.push(file);
                    }
                    e.settings_changed();
                });
            }
        }
        c if (CMD_HOTKEY..CMD_HOTKEY + Hotkey::ALL.len()).contains(&c) => {
            let hotkey = Hotkey::ALL[c - CMD_HOTKEY];
            let recording = with_engine(|e| {
                if hotkey == Hotkey::Combo {
                    e.recording = true;
                } else {
                    e.recording = false;
                    e.settings.hotkey = hotkey;
                    e.settings_changed();
                }
                e.recording
            });
            if recording == Some(true) {
                balloon(hwnd, "Своё сочетание", "Нажмите сочетание с Ctrl, Alt или Win, либо F1–F24. Esc — отмена.");
            }
        }
        CMD_BACKSPACE_UNDO => {
            with_engine(|e| {
                e.settings.backspace_undo = !e.settings.backspace_undo;
                e.settings_changed();
            });
        }
        CMD_AUTOSTART => {
            if let Err(message) = autostart::set(!autostart::enabled()) {
                balloon(hwnd, "Не удалось изменить автозапуск", &message);
            }
        }
        CMD_LISTS => {
            with_engine(|e| {
                let settings = e.settings.clone();
                e.platform.save_settings(&settings);
            });
            open("notepad.exe", Some(&settings_store::path().display().to_string()));
        }
        CMD_LICENSES => {
            if let Ok(exe) = std::env::current_exe() {
                open(&exe.with_file_name("LICENSES.txt").display().to_string(), None);
            }
        }
        CMD_QUIT => unsafe {
            Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
            PostQuitMessage(0);
        },
        _ => {}
    }
}

/// Файл настроек правили в Блокноте — перечитать.
fn reload_settings_if_changed() {
    let changed = settings_store::modified();
    with_engine(|e| {
        if changed.is_some() && changed != e.platform.saved_at {
            e.settings = settings_store::load();
            e.platform.saved_at = changed;
            e.refresh_status();
        }
    });
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            let event = (lparam & 0xFFFF) as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            0
        }
        WM_RECORDED => {
            if let Some(Some(text)) = with_engine(|e| e.settings.combo.map(|c| e.describe(c))) {
                balloon(hwnd, "Горячая клавиша", &format!("Сочетание: {text}"));
            }
            0
        }
        WM_TIMER if wparam == TIMER_SETTINGS => {
            reload_settings_if_changed();
            0
        }
        WM_DESTROY => {
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
                PostQuitMessage(0);
            }
            0
        }
        _ if msg == taskbar_created() => {
            // Проводник перезапустился — значок пропал, вернуть.
            with_engine(|e| e.refresh_status());
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn taskbar_created() -> u32 {
    use std::sync::OnceLock;
    static MSG: OnceLock<u32> = OnceLock::new();
    *MSG.get_or_init(|| unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) })
}
