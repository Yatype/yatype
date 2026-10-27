//! Список программ-исключений в окне настроек: иконка и название вместо
//! идентификаторов, выбор программы из папки «Программы».

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSImage, NSImageView, NSOpenPanel, NSTextField, NSView, NSWorkspace};
use objc2_foundation::{NSArray, NSBundle, NSFileManager, NSPoint, NSRect, NSSize, NSString, NSURL};

/// Строка списка: что хранится в настройках и как это показать.
pub struct AppRow {
    /// Запись в настройках: идентификатор (`ru.keepcoder.Telegram`) или название.
    pub entry: String,
    pub name: String,
    pub icon: Retained<NSImage>,
}

pub const ROW_HEIGHT: f64 = 24.0;

/// Найти программу по записи из настроек: сначала по идентификатору, потом
/// по названию в папках программ. Не нашлась — показываем запись как есть.
pub fn resolve(entry: &str) -> AppRow {
    let workspace = NSWorkspace::sharedWorkspace();
    let by_id = workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(entry)).and_then(|u| u.path());
    let by_name = || {
        ["/Applications", "/System/Applications", "/Applications/Utilities"]
            .iter()
            .map(|dir| NSString::from_str(&format!("{dir}/{entry}.app")))
            .find(|p| NSFileManager::defaultManager().fileExistsAtPath(p))
    };
    match by_id.or_else(by_name) {
        Some(path) => AppRow {
            entry: entry.to_string(),
            name: display_name(&path),
            icon: workspace.iconForFile(&path),
        },
        None => AppRow {
            entry: entry.to_string(),
            name: entry.to_string(),
            icon: workspace.iconForFile(&NSString::from_str("/Applications")),
        },
    }
}

/// Название программы, как его показывает Finder («Заметки», без «.app»).
fn display_name(path: &NSString) -> String {
    let name = NSFileManager::defaultManager().displayNameAtPath(path).to_string();
    name.strip_suffix(".app").map(str::to_string).unwrap_or(name)
}

/// Спросить, какие программы добавить. Возвращает записи для настроек:
/// идентификатор программы, а если его нет — название.
#[allow(deprecated)] // setAllowedFileTypes: замена требует отдельного фреймворка UTType
pub fn pick_apps(mtm: MainThreadMarker, message: &str) -> Vec<String> {
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(true);
    panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(&NSString::from_str("/Applications"))));
    panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&[NSString::from_str("app")])));
    panel.setPrompt(Some(&NSString::from_str("Добавить")));
    panel.setMessage(Some(&NSString::from_str(message)));
    if panel.runModal() != 1 {
        return Vec::new(); // NSModalResponseOK
    }
    panel
        .URLs()
        .iter()
        .filter_map(|url| {
            let id = NSBundle::bundleWithURL(&url).and_then(|b| b.bundleIdentifier());
            match id {
                Some(id) => Some(id.to_string()),
                None => url.path().map(|p| display_name(&p)),
            }
        })
        .collect()
}

/// Вид строки: иконка и название.
pub fn row_view(mtm: MainThreadMarker, row: &AppRow, width: f64) -> Retained<NSView> {
    let view = NSView::initWithFrame(NSView::alloc(mtm), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, ROW_HEIGHT)));
    let icon = NSImageView::imageViewWithImage(&row.icon, mtm);
    icon.setFrame(NSRect::new(NSPoint::new(4.0, 2.0), NSSize::new(20.0, 20.0)));
    view.addSubview(&icon);
    let label = NSTextField::labelWithString(&NSString::from_str(&row.name), mtm);
    label.setFrame(NSRect::new(NSPoint::new(30.0, 3.0), NSSize::new(width - 34.0, 18.0)));
    view.addSubview(&label);
    view
}
