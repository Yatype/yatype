//! Интерфейс: меню в строке меню, окно настроек, экран первого запуска,
//! автозапуск при входе в систему.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject, Sel};
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSAppearance, NSSwitch, NSControlSize, NSBox, NSBoxType, NSColor, NSImageView, NSTextAlignment, NSBorderType, NSControlTextEditingDelegate, NSScrollView, NSTableColumn, NSTableView, NSTableViewDataSource, NSTableViewDelegate, NSAppearanceNameAqua, NSApplication, NSBackingStoreType, NSBitmapImageFileType, NSBitmapImageRep, NSDeviceRGBColorSpace, NSButton, NSFont, NSMenu, NSMenuDelegate, NSMenuItem,
    NSPopUpButton, NSStatusItem, NSTextDelegate, NSTextField, NSTextView, NSTextViewDelegate, NSView, NSWindow, NSWindowDelegate, NSWindowStyleMask,
    NSWorkspace,
};
use objc2_foundation::{
    NSBundle, NSDictionary, NSEdgeInsets, NSError, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};

use detector::model::normalize;
use switcher::macos::ffi::*;
use engine::Settings;
use engine::settings::{Hotkey, app_excluded, parse_lines};
use switcher::macos::{guard, tap};

use super::App;
use super::app_list::{self, AppRow};

// Загружает фреймворк, чтобы был доступен класс SMAppService (автозапуск).
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

/// Страница загрузок: туда ведёт пункт «Обновления…».
const RELEASES_URL: &str = "https://github.com/Yatype/yatype/releases/latest";

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

/// Пункты меню, которые меняются.
struct MenuItems {
    enabled: Retained<NSMenuItem>,
    app: Retained<NSMenuItem>,
    access: Retained<NSMenuItem>,
}

/// Элементы окна настроек.
struct SettingsWindow {
    window: Retained<NSWindow>,
    enabled: Retained<NSSwitch>,
    login: Option<Retained<NSSwitch>>,
    hotkey: Retained<NSPopUpButton>,
    combo_label: Retained<NSTextField>,
    combo_button: Retained<NSButton>,
    backspace: Retained<NSSwitch>,
    apps_mode: Retained<NSSwitch>,
    apps_hint: Retained<NSTextField>,
    apps_empty: Retained<NSTextField>,
    apps: Retained<NSTableView>,
    words: Retained<NSTextView>,
}

pub struct Ivars {
    app: Rc<RefCell<App>>,
    menu: RefCell<Option<MenuItems>>,
    settings: RefCell<Option<SettingsWindow>>,
    onboarding: RefCell<Option<Retained<NSWindow>>>,
    about: RefCell<Option<Retained<NSWindow>>>,
    /// Приложение, которое было активным, когда открыли меню.
    target_app: RefCell<Option<(String, String)>>,
    /// Строки списка программ-исключений в окне настроек.
    app_rows: RefCell<Vec<AppRow>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "YatypeController"]
    #[ivars = Ivars]
    pub struct Controller;

    impl Controller {
        #[unsafe(method(toggleEnabled:))]
        fn toggle_enabled(&self, _sender: Option<&AnyObject>) {
            {
                let mut app = self.ivars().app.borrow_mut();
                app.settings.enabled = !app.settings.enabled;
                app.settings_changed();
            }
            self.refresh();
        }

        #[unsafe(method(toggleAppExclusion:))]
        fn toggle_app_exclusion(&self, _sender: Option<&AnyObject>) {
            let Some((id, name)) = self.ivars().target_app.borrow().clone() else { return };
            {
                let mut app = self.ivars().app.borrow_mut();
                let list = app_list_mut(&mut app.settings);
                // Программа могла попасть в список и под названием, вписанным вручную.
                let before = list.len();
                list.retain(|a| !app_excluded(std::slice::from_ref(a), &[&id, &name]));
                if list.len() == before {
                    list.push(id);
                }
                app.settings_changed();
            }
            self.fill_lists();
            self.refresh();
        }

        #[unsafe(method(showSettings:))]
        fn show_settings(&self, _sender: Option<&AnyObject>) {
            let mtm = self.mtm();
            if self.ivars().settings.borrow().is_none() {
                let w = self.build_settings_window(mtm);
                *self.ivars().settings.borrow_mut() = Some(w);
            }
            // Списки сохраняются при каждой правке, так что показать
            // сохранённое можно без потерь. Без этого окно открывалось
            // пустым и при закрытии стирало исключения.
            self.fill_lists();
            self.refresh();
            if let Some(w) = self.ivars().settings.borrow().as_ref() {
                bring_forward(mtm, &w.window);
            }
        }

        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            let mtm = self.mtm();
            if self.ivars().about.borrow().is_none() {
                let w = self.build_about_window(mtm);
                *self.ivars().about.borrow_mut() = Some(w);
            }
            if let Some(w) = self.ivars().about.borrow().as_ref() {
                bring_forward(mtm, w);
            }
        }

        #[unsafe(method(showLicenses:))]
        fn show_licenses(&self, _sender: Option<&AnyObject>) {
            let bundled = NSBundle::mainBundle().pathForResource_ofType(Some(&ns("LICENSES")), Some(&ns("txt")));
            let path = bundled
                .map(|p| p.to_string())
                .unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/LICENSES.txt").to_string());
            NSWorkspace::sharedWorkspace().openURL(&NSURL::fileURLWithPath(&ns(&path)));
        }

        /// Программа сама в сеть не ходит: только говорит, какая версия
        /// стоит, и по кнопке открывает страницу загрузок в браузере.
        #[unsafe(method(showUpdates:))]
        fn show_updates(&self, _sender: Option<&AnyObject>) {
            let mtm = self.mtm();
            let alert = NSAlert::new(mtm);
            alert.setMessageText(&ns("Обновления"));
            alert.setInformativeText(&ns(&format!(
                "У вас Yatype {}.\n\nНовые версии выходят на странице загрузок на GitHub. Если там номер больше — скачайте и поставьте поверх старой, настройки сохранятся.\n\nСам Yatype в интернет не ходит.",
                env!("CARGO_PKG_VERSION")
            )));
            alert.addButtonWithTitle(&ns("Открыть страницу загрузок"));
            alert.addButtonWithTitle(&ns("Закрыть"));
            activate(&NSApplication::sharedApplication(mtm));
            if alert.runModal() == 1000 {
                // NSAlertFirstButtonReturn
                if let Some(url) = NSURL::URLWithString(&ns(RELEASES_URL)) {
                    NSWorkspace::sharedWorkspace().openURL(&url);
                }
            }
        }

        #[unsafe(method(showOnboarding:))]
        fn show_onboarding_action(&self, _sender: Option<&AnyObject>) {
            self.show_onboarding();
        }

        #[unsafe(method(openAccessibility:))]
        fn open_accessibility(&self, _sender: Option<&AnyObject>) {
            tap::request_permission();
            let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
            if let Some(url) = NSURL::URLWithString(&ns(url)) {
                NSWorkspace::sharedWorkspace().openURL(&url);
            }
        }

        #[unsafe(method(enabledChanged:))]
        fn enabled_changed(&self, sender: Option<&AnyObject>) {
            let on = is_on(sender);
            {
                let mut app = self.ivars().app.borrow_mut();
                app.settings.enabled = on;
                app.settings_changed();
            }
            self.refresh();
        }

        #[unsafe(method(loginChanged:))]
        fn login_changed(&self, sender: Option<&AnyObject>) {
            let on = is_on(sender);
            if let Err(message) = set_login_item(on) {
                show_alert(self.mtm(), "Не удалось изменить автозапуск", &message);
            }
            self.refresh();
        }

        #[unsafe(method(hotkeyChanged:))]
        fn hotkey_changed(&self, sender: Option<&AnyObject>) {
            let index = sender.and_then(|s| s.downcast_ref::<NSPopUpButton>()).map_or(0, |p| p.indexOfSelectedItem());
            {
                let mut app = self.ivars().app.borrow_mut();
                let hotkey = Hotkey::ALL[index.clamp(0, Hotkey::ALL.len() as isize - 1) as usize];
                app.recording = false;
                if hotkey == Hotkey::Combo && app.settings.combo.is_none() {
                    // Сочетания ещё нет — сразу предлагаем его нажать.
                    app.recording = true;
                } else {
                    app.settings.hotkey = hotkey;
                    app.settings_changed();
                }
            }
            self.refresh();
        }

        #[unsafe(method(recordCombo:))]
        fn record_combo(&self, _sender: Option<&AnyObject>) {
            {
                let mut app = self.ivars().app.borrow_mut();
                app.recording = !app.recording;
            }
            self.refresh();
        }

        #[unsafe(method(backspaceUndoChanged:))]
        fn backspace_undo_changed(&self, sender: Option<&AnyObject>) {
            let on = is_on(sender);
            {
                let mut app = self.ivars().app.borrow_mut();
                app.settings.backspace_undo = on;
                app.settings_changed();
            }
            self.refresh();
        }

        #[unsafe(method(appsModeChanged:))]
        fn apps_mode_changed(&self, sender: Option<&AnyObject>) {
            {
                let mut app = self.ivars().app.borrow_mut();
                app.settings.only_listed = is_on(sender);
                app.settings_changed();
            }
            self.fill_lists();
            self.refresh();
        }

        #[unsafe(method(addApps:))]
        fn add_apps(&self, _sender: Option<&AnyObject>) {
            let message = if self.ivars().app.borrow().settings.only_listed {
                "Выберите программы, где Yatype будет исправлять раскладку"
            } else {
                "Выберите программы, где Yatype не будет исправлять раскладку"
            };
            let picked = app_list::pick_apps(self.mtm(), message);
            if picked.is_empty() {
                return;
            }
            {
                let mut app = self.ivars().app.borrow_mut();
                for entry in picked {
                    let list = app_list_mut(&mut app.settings);
                    if !app_excluded(list, &[&entry]) {
                        list.push(entry);
                    }
                }
                app.settings_changed();
            }
            self.fill_lists();
            self.refresh();
        }

        #[unsafe(method(removeApp:))]
        fn remove_app(&self, _sender: Option<&AnyObject>) {
            let row = match self.ivars().settings.borrow().as_ref() {
                Some(w) => w.apps.selectedRow(),
                None => return,
            };
            let Some(entry) = usize::try_from(row).ok().and_then(|i| self.ivars().app_rows.borrow().get(i).map(|r| r.entry.clone()))
            else {
                return;
            };
            {
                let mut app = self.ivars().app.borrow_mut();
                app_list_mut(&mut app.settings).retain(|a| *a != entry);
                app.settings_changed();
            }
            self.fill_lists();
            self.refresh();
        }
    }

    unsafe impl NSObjectProtocol for Controller {}

    unsafe impl NSMenuDelegate for Controller {
        #[unsafe(method(menuWillOpen:))]
        fn menu_will_open(&self, _menu: &NSMenu) {
            // Меню в строке меню не делает нашу программу активной, так что
            // активным осталось приложение, в котором пользователь работал.
            let own = NSBundle::mainBundle().bundleIdentifier().map(|s| s.to_string());
            let front = guard::frontmost_app_with_name()
                .filter(|(id, _)| Some(id) != own.as_ref() && !guard::always_excluded(id));
            *self.ivars().target_app.borrow_mut() = front;
            self.refresh();
        }
    }

    unsafe impl NSWindowDelegate for Controller {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            self.save_lists();
        }
    }

    // Списки сохраняются сразу при правке: иначе изменения пропали бы, если
    // выйти из программы, не закрыв окно настроек.
    unsafe impl NSTextDelegate for Controller {
        #[unsafe(method(textDidChange:))]
        fn text_did_change(&self, _notification: &NSNotification) {
            self.save_lists();
        }
    }

    unsafe impl NSTextViewDelegate for Controller {}

    unsafe impl NSTableViewDataSource for Controller {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _table: &NSTableView) -> isize {
            self.ivars().app_rows.borrow().len() as isize
        }
    }

    unsafe impl NSControlTextEditingDelegate for Controller {}

    unsafe impl NSTableViewDelegate for Controller {
        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn view_for_row(&self, table: &NSTableView, _column: Option<&NSTableColumn>, row: isize) -> Option<Retained<NSView>> {
            let rows = self.ivars().app_rows.borrow();
            let row = usize::try_from(row).ok().and_then(|i| rows.get(i));
            row.map(|r| app_list::row_view(self.mtm(), r, table.bounds().size.width))
        }
    }
);

impl Controller {
    pub fn new(mtm: MainThreadMarker, app: Rc<RefCell<App>>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            app,
            menu: RefCell::new(None),
            settings: RefCell::new(None),
            onboarding: RefCell::new(None),
            about: RefCell::new(None),
            target_app: RefCell::new(None),
            app_rows: RefCell::new(Vec::new()),
        });
        unsafe { msg_send![super(this), init] }
    }

    fn target(&self) -> &AnyObject {
        self
    }

    fn item(&self, title: &str, action: Option<Sel>, key: &str) -> Retained<NSMenuItem> {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(self.mtm().alloc(), &ns(title), action, &ns(key))
        };
        unsafe { item.setTarget(Some(self.target())) };
        item
    }

    /// Меню значка в строке меню.
    pub fn build_menu(&self, status: &NSStatusItem, last: &NSMenuItem) {
        let mtm = self.mtm();
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        menu.setDelegate(Some(ProtocolObject::from_ref(self)));

        last.setEnabled(false);
        last.setHidden(true);
        menu.addItem(last);
        let access = self.item("⚠︎ Выдать доступ…", Some(sel!(showOnboarding:)), "");
        menu.addItem(&access);
        menu.addItem(&NSMenuItem::separatorItem(mtm));

        let enabled = self.item("Приостановить Yatype", Some(sel!(toggleEnabled:)), "");
        menu.addItem(&enabled);
        let app = self.item("Не исправлять в этом приложении", Some(sel!(toggleAppExclusion:)), "");
        menu.addItem(&app);
        menu.addItem(&NSMenuItem::separatorItem(mtm));

        menu.addItem(&self.item("Настройки…", Some(sel!(showSettings:)), ","));
        menu.addItem(&self.item("Обновления…", Some(sel!(showUpdates:)), ""));
        menu.addItem(&self.item("О Yatype", Some(sel!(showAbout:)), ""));
        menu.addItem(&NSMenuItem::separatorItem(mtm));

        let quit = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &ns("Выход"), Some(sel!(terminate:)), &ns("q"))
        };
        let nsapp: &AnyObject = &NSApplication::sharedApplication(mtm);
        unsafe { quit.setTarget(Some(nsapp)) };
        menu.addItem(&quit);

        status.setMenu(Some(&menu));
        *self.ivars().menu.borrow_mut() = Some(MenuItems { enabled, app, access });
        self.refresh();
    }

    /// Привести меню и окно настроек в соответствие с настройками.
    fn refresh(&self) {
        self.ivars().app.borrow_mut().refresh_status();
        let app = self.ivars().app.borrow();
        let s = &app.settings;
        if let Some(m) = self.ivars().menu.borrow().as_ref() {
            m.enabled.setTitle(&ns(if s.enabled { "Приостановить Yatype" } else { "▶ Включить Yatype" }));
            m.access.setHidden(app.running);
            match self.ivars().target_app.borrow().as_ref() {
                Some((id, name)) => {
                    let verb = if s.only_listed { "Исправлять" } else { "Не исправлять" };
                    m.app.setTitle(&ns(&format!("{verb} в «{name}»")));
                    m.app.setState(app_excluded(app_list(s), &[id, name]) as isize);
                    m.app.setHidden(false);
                }
                None => m.app.setHidden(true),
            }
        }
        if let Some(w) = self.ivars().settings.borrow().as_ref() {
            w.enabled.setState(s.enabled as isize);
            if let Some(login) = &w.login {
                login.setState(login_item_enabled() as isize);
            }
            let shown = if app.recording { Hotkey::Combo } else { s.hotkey };
            let index = Hotkey::ALL.iter().position(|h| *h == shown).unwrap_or(0);
            w.hotkey.selectItemAtIndex(index as isize);
            let combo_text = if app.recording {
                "Нажмите клавиши…".to_string()
            } else {
                match s.combo {
                    Some(c) => app.describe(c),
                    None => "не задано".to_string(),
                }
            };
            w.combo_label.setStringValue(&ns(&combo_text));
            w.combo_button.setTitle(&ns(if app.recording { "Отмена" } else { "Записать…" }));
            let combo_row = shown == Hotkey::Combo;
            w.combo_label.setHidden(!combo_row);
            w.combo_button.setHidden(!combo_row);
            w.backspace.setState(s.backspace_undo as isize);
        }
    }

    /// Показать в окне настроек сохранённые списки (затирает несохранённые правки).
    fn fill_lists(&self) {
        let app = self.ivars().app.borrow();
        if let Some(w) = self.ivars().settings.borrow().as_ref() {
            *self.ivars().app_rows.borrow_mut() = app_list(&app.settings).iter().map(|e| app_list::resolve(e)).collect();
            w.apps_mode.setState(app.settings.only_listed as isize);
            let (hint, warn) = apps_hint(&app.settings);
            w.apps_hint.setStringValue(&ns(hint));
            let color = if warn { NSColor::systemRedColor() } else { NSColor::secondaryLabelColor() };
            w.apps_hint.setTextColor(Some(&color));
            w.apps_empty.setHidden(!app_list(&app.settings).is_empty());
            w.apps.reloadData();
            w.words.setString(&ns(&app.settings.exception_words.join("\n")));
        }
    }

    fn save_lists(&self) {
        let Some(words) = self.ivars().settings.borrow().as_ref().map(|w| w.words.string().to_string()) else {
            return;
        };
        let mut app = self.ivars().app.borrow_mut();
        app.settings.exception_words = parse_lines(&words).iter().map(|w| normalize(w)).collect();
        app.settings_changed();
    }

    /// Настройки в духе «Системных настроек» macOS: скруглённые группы
    /// строк, подпись слева, переключатель или список справа, короткие
    /// пояснения под группами.
    fn build_settings_window(&self, mtm: MainThreadMarker) -> SettingsWindow {
        let (width, height) = (540.0, 1000.0);
        let (left, inner) = (20.0, width - 40.0);
        let window = new_window(mtm, "Настройки Yatype", width, height);
        window.setDelegate(Some(ProtocolObject::from_ref(self)));
        let content = window.contentView().expect("есть содержимое окна");
        let mut y = height - 20.0;

        const ROW: f64 = 38.0;
        const PAD: f64 = 12.0;
        let add = |view: &NSView, x: f64, y: f64, w: f64, h: f64| {
            view.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)));
            content.addSubview(view);
        };
        // Заголовок группы — жирный, как в «Системных настройках».
        let header = |y: &mut f64, text: &str| {
            let label = NSTextField::labelWithString(&ns(text), mtm);
            label.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
            *y -= 17.0;
            add(&label, left + 4.0, *y, inner - 8.0, 17.0);
            *y -= 8.0;
        };
        // Фон группы: скруглённый прямоугольник с тонкой рамкой.
        let group = |top: f64, h: f64| {
            let b = NSBox::new(mtm);
            b.setBoxType(NSBoxType::Custom);
            b.setCornerRadius(10.0);
            b.setBorderWidth(0.5);
            b.setBorderColor(&NSColor::separatorColor());
            b.setFillColor(&NSColor::controlBackgroundColor());
            add(&b, left, top - h, inner, h);
        };
        let divider = |y: f64| {
            let line = NSBox::new(mtm);
            line.setBoxType(NSBoxType::Separator);
            add(&line, left + PAD, y, inner - 2.0 * PAD, 1.0);
        };
        let label = |text: &str| NSTextField::labelWithString(&ns(text), mtm);
        // Строка: подпись слева, элемент справа (по вертикали — посередине строки).
        let row = |top: f64, text: &str, control: &NSView, w: f64, h: f64| {
            add(&label(text), left + PAD, top - ROW + (ROW - 17.0) / 2.0, inner - w - 3.0 * PAD, 17.0);
            add(control, left + inner - PAD - w, top - ROW + (ROW - h) / 2.0, w, h);
        };
        let new_switch = |action: Sel| {
            let sw = NSSwitch::new(mtm);
            unsafe {
                sw.setTarget(Some(self.target()));
                sw.setAction(Some(action));
            }
            sw
        };
        let caption = |y: &mut f64, text: &str| {
            let label = NSTextField::wrappingLabelWithString(&ns(text), mtm);
            label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
            label.setTextColor(Some(&NSColor::secondaryLabelColor()));
            label.setPreferredMaxLayoutWidth(inner - 16.0);
            let h = label.fittingSize().height.ceil();
            *y -= 6.0 + h;
            add(&label, left + 8.0, *y, inner - 16.0, h);
            label
        };

        // Основное: Yatype работает, автозапуск.
        let enabled = new_switch(sel!(enabledChanged:));
        let login = login_service().map(|_| new_switch(sel!(loginChanged:)));
        let rows = if login.is_some() { 2.0 } else { 1.0 };
        group(y, ROW * rows);
        row(y, "Yatype работает", &enabled, 40.0, 22.0);
        if let Some(login) = &login {
            divider(y - ROW);
            row(y - ROW, "Запускать при входе в систему", login, 40.0, 22.0);
        }
        y -= ROW * rows + 22.0;

        // Отмена и ручной перевод.
        header(&mut y, "Отмена и ручной перевод");
        let hotkey = NSPopUpButton::initWithFrame_pullsDown(mtm.alloc(), NSRect::ZERO, false);
        for h in Hotkey::ALL {
            hotkey.addItemWithTitle(&ns(h.title()));
        }
        unsafe {
            hotkey.setTarget(Some(self.target()));
            hotkey.setAction(Some(sel!(hotkeyChanged:)));
        }
        let backspace = new_switch(sel!(backspaceUndoChanged:));
        group(y, ROW * 2.0);
        row(y, "Клавиша", &hotkey, 216.0, 26.0);
        // Своё сочетание — в той же строке, левее списка; видно, только когда выбрано.
        let combo_button = unsafe {
            NSButton::buttonWithTitle_target_action(&ns("Записать…"), Some(self.target()), Some(sel!(recordCombo:)), mtm)
        };
        combo_button.setControlSize(NSControlSize::Small);
        let combo_x = left + inner - PAD - 216.0 - 8.0 - 84.0;
        add(&combo_button, combo_x, y - ROW + (ROW - 24.0) / 2.0, 84.0, 24.0);
        let combo_label = label("");
        combo_label.setAlignment(NSTextAlignment::Right);
        combo_label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        add(&combo_label, combo_x - 108.0, y - ROW + (ROW - 17.0) / 2.0, 100.0, 17.0);
        divider(y - ROW);
        row(y - ROW, "Backspace сразу после исправления тоже отменяет", &backspace, 40.0, 22.0);
        y -= ROW * 2.0;
        caption(&mut y, "Сразу после исправления клавиша возвращает как было. Если исправлять было нечего — переводит последнее слово.");
        y -= 22.0;

        // Программы: переключатель режима и список в одной группе.
        header(&mut y, "Программы");
        let apps_mode = new_switch(sel!(appsModeChanged:));
        let list_h = 132.0;
        group(y, ROW + list_h);
        row(y, "Работать только в выбранных программах", &apps_mode, 40.0, 22.0);
        divider(y - ROW);
        let (apps_scroll, apps) = app_table(mtm, inner - 2.0);
        unsafe {
            apps.setDataSource(Some(ProtocolObject::from_ref(self)));
            apps.setDelegate(Some(ProtocolObject::from_ref(self)));
        }
        let list_bottom = y - ROW - list_h;
        add(&apps_scroll, left + 1.0, list_bottom + 1.0, inner - 2.0, list_h - 2.0);
        // Пустой список — серая надпись посередине, как в Системных настройках.
        let apps_empty = NSTextField::labelWithString(&ns("Нет программ"), mtm);
        apps_empty.setTextColor(Some(&NSColor::tertiaryLabelColor()));
        apps_empty.setAlignment(NSTextAlignment::Center);
        add(&apps_empty, left, list_bottom + 26.0 + (list_h - 26.0) / 2.0 - 9.0, inner, 18.0);
        // Полоса с «+» и «−» внизу группы, как в списках macOS.
        let strip = NSBox::new(mtm);
        strip.setBoxType(NSBoxType::Custom);
        strip.setBorderWidth(0.0);
        strip.setCornerRadius(9.0);
        strip.setFillColor(&NSColor::controlBackgroundColor());
        add(&strip, left + 1.0, list_bottom + 1.0, inner - 2.0, 26.0);
        divider(list_bottom + 27.0);
        let buttons = [("+", sel!(addApps:)), ("−", sel!(removeApp:))];
        for (i, (title, action)) in buttons.into_iter().enumerate() {
            let b = unsafe { NSButton::buttonWithTitle_target_action(&ns(title), Some(self.target()), Some(action), mtm) };
            b.setBordered(false);
            b.setFont(Some(&NSFont::systemFontOfSize(16.0)));
            add(&b, left + 8.0 + i as f64 * 26.0, list_bottom + 3.0, 24.0, 22.0);
        }
        y = list_bottom;
        // Подсказка меняется с режимом (см. `apps_hint`); высоту берём по самому длинному тексту.
        let apps_hint = caption(&mut y, APPS_HINT_EMPTY);
        y -= 22.0;

        // Слова-исключения.
        header(&mut y, "Слова, которые не исправлять");
        let words_h = 84.0;
        group(y, words_h);
        let (words_scroll, words) = text_area(mtm);
        words.setDelegate(Some(ProtocolObject::from_ref(self)));
        words.setDrawsBackground(false);
        if let Some(scroll) = words_scroll.downcast_ref::<NSScrollView>() {
            scroll.setBorderType(NSBorderType::NoBorder);
            scroll.setDrawsBackground(false);
        }
        add(&words_scroll, left + 6.0, y - words_h + 4.0, inner - 12.0, words_h - 8.0);
        y -= words_h;
        caption(&mut y, "По одному в строке. Сюда сами попадают исправления, которые вы отменили.");
        let bottom = y;

        // Высота окна — по содержимому: снизу тот же отступ, что сверху.
        let shift = bottom - 20.0;
        for view in content.subviews().iter() {
            let mut frame = view.frame();
            frame.origin.y -= shift;
            view.setFrame(frame);
        }
        window.setContentSize(NSSize::new(width, height - shift));
        window.center();
        SettingsWindow { window, enabled, login, hotkey, combo_label, combo_button, backspace, apps_mode, apps_hint, apps_empty, apps, words }
    }

    /// «О Yatype»: иконка, версия, одна строка о программе и ссылка на
    /// лицензии словарей (их уведомления обязаны быть доступны всегда).
    fn build_about_window(&self, mtm: MainThreadMarker) -> Retained<NSWindow> {
        let (width, height) = (320.0, 236.0);
        let window = new_window(mtm, "О Yatype", width, height);
        let content = window.contentView().expect("есть содержимое окна");
        let centered = |view: &NSView, y: f64, h: f64| {
            view.setFrame(NSRect::new(NSPoint::new(20.0, y), NSSize::new(width - 40.0, h)));
            content.addSubview(view);
        };
        if let Some(image) = NSApplication::sharedApplication(mtm).applicationIconImage() {
            let icon = NSImageView::imageViewWithImage(&image, mtm);
            icon.setFrame(NSRect::new(NSPoint::new((width - 64.0) / 2.0, height - 84.0), NSSize::new(64.0, 64.0)));
            content.addSubview(&icon);
        }
        let name = NSTextField::labelWithString(&ns(&format!("Yatype {}", env!("CARGO_PKG_VERSION"))), mtm);
        name.setFont(Some(&NSFont::boldSystemFontOfSize(15.0)));
        name.setAlignment(NSTextAlignment::Center);
        centered(&name, height - 112.0, 20.0);
        let about = NSTextField::wrappingLabelWithString(
            &ns("Исправляет текст, набранный не в той раскладке. Ничего не отправляет в интернет."),
            mtm,
        );
        about.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        about.setTextColor(Some(&NSColor::secondaryLabelColor()));
        about.setAlignment(NSTextAlignment::Center);
        centered(&about, height - 160.0, 36.0);
        let licenses = unsafe {
            NSButton::buttonWithTitle_target_action(&ns("Лицензии словарей"), Some(self.target()), Some(sel!(showLicenses:)), mtm)
        };
        licenses.setBordered(false);
        licenses.setContentTintColor(Some(&NSColor::linkColor()));
        centered(&licenses, height - 196.0, 20.0);
        let copyright = NSTextField::labelWithString(&ns("© 2026"), mtm);
        copyright.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        copyright.setTextColor(Some(&NSColor::tertiaryLabelColor()));
        copyright.setAlignment(NSTextAlignment::Center);
        centered(&copyright, 14.0, 16.0);
        window.center();
        window
    }

    /// Экран первого запуска: зачем нужен доступ и как его выдать.
    pub fn show_onboarding(&self) {
        let mtm = self.mtm();
        if self.ivars().onboarding.borrow().is_none() {
            let (width, height) = (480.0, 330.0);
            let window = new_window(mtm, "Yatype", width, height);
            let content = window.contentView().expect("есть содержимое окна");
            let mut y = height - 24.0;
            let mut place = |view: &NSView, h: f64, gap: f64| {
                y -= h;
                view.setFrame(NSRect::new(NSPoint::new(24.0, y), NSSize::new(width - 48.0, h)));
                content.addSubview(view);
                y -= gap;
            };
            let title = NSTextField::labelWithString(&ns("Нужно разрешение «Универсальный доступ»"), mtm);
            title.setFont(Some(&NSFont::boldSystemFontOfSize(15.0)));
            place(&title, 22.0, 12.0);
            place(
                &NSTextField::wrappingLabelWithString(
                    &ns("Программа исправляет текст, набранный не в той раскладке. Для этого ей нужно видеть нажатия клавиш и печатать исправленный текст — macOS разрешает это только через «Универсальный доступ».\n\nПрограмма ничего не отправляет в интернет и не сохраняет набранный текст. В полях паролей она не работает."),
                    mtm,
                ),
                120.0,
                12.0,
            );
            place(
                &NSTextField::wrappingLabelWithString(
                    &ns("1. Нажмите «Открыть настройки».\n2. Включите «Yatype» в списке.\n3. Готово — программа заработает сама, перезапускать не нужно."),
                    mtm,
                ),
                60.0,
                12.0,
            );
            let open = unsafe {
                NSButton::buttonWithTitle_target_action(
                    &ns("Открыть настройки"),
                    Some(self.target()),
                    Some(sel!(openAccessibility:)),
                    mtm,
                )
            };
            open.setFrame(NSRect::new(NSPoint::new(width - 204.0, 20.0), NSSize::new(180.0, 32.0)));
            content.addSubview(&open);
            window.center();
            *self.ivars().onboarding.borrow_mut() = Some(window);
        }
        if let Some(w) = self.ivars().onboarding.borrow().as_ref() {
            bring_forward(mtm, w);
        }
    }

    /// Поставить перехват, если разрешение уже есть. Возвращает, работает ли.
    /// Перехват может молча умереть: когда разрешение отозвали и выдали
    /// снова, macOS выключает его, и программа перестала бы работать везде
    /// до перезапуска. Поэтому раз в секунду проверяем и поднимаем заново.
    fn keep_tap_alive(&self) {
        if !self.ivars().app.borrow().running {
            self.start_if_permitted();
            return;
        }
        let permitted = tap::has_permission();
        if permitted && (tap::is_enabled() || tap::reenable()) {
            return;
        }
        eprintln!("Перехват клавиатуры выключен системой — ставлю заново.");
        tap::remove();
        self.ivars().app.borrow_mut().running = false;
        if !self.start_if_permitted() {
            self.refresh(); // в меню появится «Выдать доступ…»
        }
    }

    pub fn start_if_permitted(&self) -> bool {
        if self.ivars().app.borrow().running {
            return true;
        }
        if !tap::has_permission() {
            return false;
        }
        let app = self.ivars().app.clone();
        let installed = tap::install(move |input| match app.try_borrow_mut() {
            Ok(mut app) => app.on_input(input),
            Err(_) => engine::Pass::Through,
        });
        if let Err(e) = installed {
            eprintln!("Ошибка: {e}");
            return false;
        }
        self.ivars().app.borrow_mut().running = true;
        if let Some(w) = self.ivars().onboarding.borrow_mut().take() {
            w.close();
        }
        self.refresh();
        true
    }
}

thread_local! {
    static CONTROLLER: RefCell<Option<Retained<Controller>>> = const { RefCell::new(None) };
}

extern "C" fn permission_tick(_timer: *mut c_void, _info: *mut c_void) {
    CONTROLLER.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            c.keep_tap_alive();
        }
    });
}

extern "C" fn refresh_tick(_timer: *mut c_void, _info: *mut c_void) {
    CONTROLLER.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            c.refresh();
        }
    });
}

/// Обновить меню и настройки чуть позже, когда текущая обработка нажатия
/// закончится (из неё самой интерфейс трогать нельзя: состояние занято).
pub fn refresh_soon() {
    unsafe {
        let timer = CFRunLoopTimerCreate(std::ptr::null(), CFAbsoluteTimeGetCurrent(), 0.0, 0, 0, refresh_tick, std::ptr::null());
        CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
    }
}

/// Раз в секунду проверять, не выдали ли разрешение (тогда заработать без
/// перезапуска) и жив ли перехват.
pub fn watch_permission(controller: Retained<Controller>) {
    CONTROLLER.with(|c| *c.borrow_mut() = Some(controller));
    unsafe {
        let timer =
            CFRunLoopTimerCreate(std::ptr::null(), CFAbsoluteTimeGetCurrent() + 1.0, 1.0, 0, 0, permission_tick, std::ptr::null());
        CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
    }
}

#[allow(deprecated)] // activateIgnoringOtherApps: нужно, чтобы окно программы из строки меню вышло вперёд
fn activate(app: &NSApplication) {
    app.activateIgnoringOtherApps(true);
}

fn bring_forward(mtm: MainThreadMarker, window: &NSWindow) {
    activate(&NSApplication::sharedApplication(mtm));
    window.makeKeyAndOrderFront(None);
}

fn new_window(mtm: MainThreadMarker, title: &str, width: f64, height: f64) -> Retained<NSWindow> {
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&ns(title));
    window
}

const APPS_HINT_EMPTY: &str =
    "Список пуст — сейчас Yatype не работает ни в одной программе. Добавьте программы кнопкой «+» или выключите переключатель выше.";

/// Подсказка под списком программ: что значит текущий режим. `true` —
/// предупреждение: в режиме «только в этих» с пустым списком Yatype молчит везде.
fn apps_hint(s: &Settings) -> (&'static str, bool) {
    match (s.only_listed, s.allowed_apps.is_empty()) {
        (false, _) => ("Yatype работает во всех программах, кроме этих. Терминалы и менеджеры паролей исключены всегда.", false),
        (true, false) => ("Yatype работает только в этих программах, в остальных молчит.", false),
        (true, true) => (APPS_HINT_EMPTY, true),
    }
}

/// Включён ли переключатель, приславший действие.
fn is_on(sender: Option<&AnyObject>) -> bool {
    sender.and_then(|s| s.downcast_ref::<NSSwitch>()).is_some_and(|b| b.state() == 1)
}

/// Список программ, который сейчас действует: исключения или, в режиме
/// «только в этих программах», список разрешённых.
fn app_list(s: &Settings) -> &Vec<String> {
    if s.only_listed { &s.allowed_apps } else { &s.excluded_apps }
}

fn app_list_mut(s: &mut Settings) -> &mut Vec<String> {
    if s.only_listed { &mut s.allowed_apps } else { &mut s.excluded_apps }
}

/// Список программ: одна колонка без заголовка, строки с иконками.
fn app_table(mtm: MainThreadMarker, width: f64) -> (Retained<NSView>, Retained<NSTableView>) {
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), NSRect::ZERO);
    let table = NSTableView::initWithFrame(NSTableView::alloc(mtm), NSRect::ZERO);
    let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &ns("app"));
    column.setWidth(width - 4.0);
    table.addTableColumn(&column);
    table.setHeaderView(None);
    table.setRowHeight(app_list::ROW_HEIGHT);
    table.setAllowsMultipleSelection(false);
    scroll.setDocumentView(Some(&table));
    // Рамку рисует группа, в которой лежит список.
    scroll.setBorderType(NSBorderType::NoBorder);
    table.setBackgroundColor(&NSColor::controlBackgroundColor());
    scroll.setHasVerticalScroller(true);
    // Внизу место под кнопки «+» и «−»: последняя строка и полоса
    // прокрутки не уходят под них.
    let bottom = NSEdgeInsets { top: 0.0, left: 0.0, bottom: 26.0, right: 0.0 };
    scroll.setAutomaticallyAdjustsContentInsets(false);
    scroll.setContentInsets(bottom);
    scroll.setScrollerInsets(bottom);
    (Retained::into_super(scroll), table)
}

/// Многострочное поле для списков, без автозамен.
fn text_area(mtm: MainThreadMarker) -> (Retained<NSView>, Retained<NSTextView>) {
    let scroll = NSTextView::scrollableTextView(mtm);
    let text = scroll
        .documentView()
        .and_then(|v| v.downcast::<NSTextView>().ok())
        .expect("в scrollableTextView лежит NSTextView");
    text.setRichText(false);
    text.setEnabledTextCheckingTypes(0);
    text.setFont(Some(&NSFont::userFixedPitchFontOfSize(12.0).unwrap_or_else(|| NSFont::systemFontOfSize(12.0))));
    scroll.setBorderType(NSBorderType::BezelBorder);
    scroll.setHasVerticalScroller(true);
    (Retained::into_super(scroll), text)
}

/// Отладка без разрешений на запись экрана: открыть окна, через секунду
/// (когда macOS их нарисует) сохранить в PNG и выйти.
pub fn snapshot_windows(controller: Retained<Controller>, dir: String) {
    // Светлое оформление: в снимок попадает только содержимое окна без его
    // фона, и белый текст тёмной темы на нём не виден.
    let light = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua });
    NSApplication::sharedApplication(controller.mtm()).setAppearance(light.as_deref());
    controller.show_onboarding();
    // Настройки открываем последними: в активном окне переключатели цветные.
    let _: () = unsafe { msg_send![&*controller, showAbout: std::ptr::null::<AnyObject>()] };
    let _: () = unsafe { msg_send![&*controller, showSettings: std::ptr::null::<AnyObject>()] };
    SNAPSHOT_DIR.with(|d| *d.borrow_mut() = Some(dir));
    CONTROLLER.with(|c| *c.borrow_mut() = Some(controller));
    unsafe {
        let timer =
            CFRunLoopTimerCreate(std::ptr::null(), CFAbsoluteTimeGetCurrent() + 1.0, 0.0, 0, 0, snapshot_tick, std::ptr::null());
        CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
    }
}

thread_local! {
    static SNAPSHOT_DIR: RefCell<Option<String>> = const { RefCell::new(None) };
}

extern "C" fn snapshot_tick(_timer: *mut c_void, _info: *mut c_void) {
    let dir = SNAPSHOT_DIR.with(|d| d.borrow().clone()).unwrap_or_else(|| ".".into());
    CONTROLLER.with(|c| {
        let c = c.borrow();
        let Some(controller) = c.as_ref() else { return };
        let windows = [
            ("onboarding", controller.ivars().onboarding.borrow().as_ref().cloned()),
            ("settings", controller.ivars().settings.borrow().as_ref().map(|w| w.window.clone())),
            ("about", controller.ivars().about.borrow().as_ref().cloned()),
        ];
        for (name, window) in windows {
            let Some(view) = window.and_then(|w| w.contentView()) else { continue };
            let rect = view.bounds();
            // Вдвое крупнее, как на экране Retina, на любом мониторе.
            let bitmap = unsafe {
                NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                    NSBitmapImageRep::alloc(),
                    std::ptr::null_mut(),
                    (rect.size.width * 2.0) as isize,
                    (rect.size.height * 2.0) as isize,
                    8,
                    4,
                    true,
                    false,
                    NSDeviceRGBColorSpace,
                    0,
                    0,
                )
            };
            let Some(bitmap) = bitmap else { continue };
            bitmap.setSize(rect.size);
            view.cacheDisplayInRect_toBitmapImageRep(rect, &bitmap);
            let png = unsafe {
                bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
            };
            if let Some(png) = png {
                png.writeToFile_atomically(&ns(&format!("{dir}/{name}.png")), true);
            }
        }
    });
    std::process::exit(0);
}

pub fn show_alert(mtm: MainThreadMarker, title: &str, text: &str) {
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&ns(title));
    alert.setInformativeText(&ns(text));
    activate(&NSApplication::sharedApplication(mtm));
    alert.runModal();
}

// ---------- Автозапуск (SMAppService, macOS 13+) ----------

/// Служба автозапуска. `None` — недоступна: старая macOS или программа
/// запущена не как .app.
fn login_service() -> Option<Retained<AnyObject>> {
    NSBundle::mainBundle().bundleIdentifier()?;
    let class = AnyClass::get(c"SMAppService")?;
    Some(unsafe { msg_send![class, mainAppService] })
}

fn login_item_enabled() -> bool {
    login_service().is_some_and(|s| {
        let status: isize = unsafe { msg_send![&*s, status] };
        status == 1 // SMAppServiceStatusEnabled
    })
}

fn set_login_item(on: bool) -> Result<(), String> {
    let Some(service) = login_service() else { return Err("Автозапуск доступен с macOS 13.".into()) };
    let result: Result<(), Retained<NSError>> = unsafe {
        if on {
            msg_send![&*service, registerAndReturnError: _]
        } else {
            msg_send![&*service, unregisterAndReturnError: _]
        }
    };
    result.map_err(|e| e.localizedDescription().to_string())
}
