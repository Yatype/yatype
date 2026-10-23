//! Интерфейс: меню в строке меню, окно настроек, экран первого запуска,
//! автозапуск при входе в систему.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSAppearance, NSBorderType, NSAppearanceNameAqua, NSApplication, NSBackingStoreType, NSBitmapImageFileType, NSButton, NSFont, NSMenu, NSMenuDelegate, NSMenuItem,
    NSPopUpButton, NSStatusItem, NSTextField, NSTextView, NSView, NSWindow, NSWindowDelegate, NSWindowStyleMask,
    NSWorkspace,
};
use objc2_foundation::{
    NSBundle, NSDictionary, NSError, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};

use detector::model::normalize;
use switcher::macos::ffi::*;
use engine::settings::{Hotkey, parse_lines};
use switcher::macos::{guard, tap};

use super::App;

// Загружает фреймворк, чтобы был доступен класс SMAppService (автозапуск).
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

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
    enabled: Retained<NSButton>,
    login: Option<Retained<NSButton>>,
    hotkey: Retained<NSPopUpButton>,
    combo_label: Retained<NSTextField>,
    combo_button: Retained<NSButton>,
    backspace: Retained<NSButton>,
    apps: Retained<NSTextView>,
    words: Retained<NSTextView>,
}

pub struct Ivars {
    app: Rc<RefCell<App>>,
    menu: RefCell<Option<MenuItems>>,
    settings: RefCell<Option<SettingsWindow>>,
    onboarding: RefCell<Option<Retained<NSWindow>>>,
    /// Приложение, которое было активным, когда открыли меню.
    target_app: RefCell<Option<(String, String)>>,
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
            let Some((id, _)) = self.ivars().target_app.borrow().clone() else { return };
            {
                let mut app = self.ivars().app.borrow_mut();
                let list = &mut app.settings.excluded_apps;
                if let Some(i) = list.iter().position(|a| *a == id) {
                    list.remove(i);
                } else {
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
            self.refresh();
            if let Some(w) = self.ivars().settings.borrow().as_ref() {
                bring_forward(mtm, &w.window);
            }
        }

        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            let app = NSApplication::sharedApplication(self.mtm());
            activate(&app);
            app.orderFrontStandardAboutPanel(None);
        }

        #[unsafe(method(showLicenses:))]
        fn show_licenses(&self, _sender: Option<&AnyObject>) {
            let bundled = NSBundle::mainBundle().pathForResource_ofType(Some(&ns("LICENSES")), Some(&ns("txt")));
            let path = bundled
                .map(|p| p.to_string())
                .unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/LICENSES.txt").to_string());
            NSWorkspace::sharedWorkspace().openURL(&NSURL::fileURLWithPath(&ns(&path)));
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
            let on = sender.and_then(|s| s.downcast_ref::<NSButton>()).is_some_and(|b| b.state() == 1);
            {
                let mut app = self.ivars().app.borrow_mut();
                app.settings.enabled = on;
                app.settings_changed();
            }
            self.refresh();
        }

        #[unsafe(method(loginChanged:))]
        fn login_changed(&self, sender: Option<&AnyObject>) {
            let on = sender.and_then(|s| s.downcast_ref::<NSButton>()).is_some_and(|b| b.state() == 1);
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
            let on = sender.and_then(|s| s.downcast_ref::<NSButton>()).is_some_and(|b| b.state() == 1);
            {
                let mut app = self.ivars().app.borrow_mut();
                app.settings.backspace_undo = on;
                app.settings_changed();
            }
            self.refresh();
        }

        #[unsafe(method(saveLists:))]
        fn save_lists_action(&self, _sender: Option<&AnyObject>) {
            self.save_lists();
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
);

impl Controller {
    pub fn new(mtm: MainThreadMarker, app: Rc<RefCell<App>>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            app,
            menu: RefCell::new(None),
            settings: RefCell::new(None),
            onboarding: RefCell::new(None),
            target_app: RefCell::new(None),
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

        let enabled = self.item("Исправлять автоматически", Some(sel!(toggleEnabled:)), "");
        menu.addItem(&enabled);
        let app = self.item("Не исправлять в этом приложении", Some(sel!(toggleAppExclusion:)), "");
        menu.addItem(&app);
        menu.addItem(&NSMenuItem::separatorItem(mtm));

        menu.addItem(&self.item("Настройки…", Some(sel!(showSettings:)), ","));
        menu.addItem(&self.item("О программе", Some(sel!(showAbout:)), ""));
        menu.addItem(&self.item("Лицензии словарей", Some(sel!(showLicenses:)), ""));
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
            m.enabled.setState(s.enabled as isize);
            m.access.setHidden(app.running);
            match self.ivars().target_app.borrow().as_ref() {
                Some((id, name)) => {
                    m.app.setTitle(&ns(&format!("Не исправлять в «{name}»")));
                    m.app.setState(s.excluded_apps.contains(id) as isize);
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
                "Нажмите сочетание… (Esc — отмена)".to_string()
            } else {
                match s.combo {
                    Some(c) => format!("Сочетание: {}", app.describe(c)),
                    None => "Сочетание не задано".to_string(),
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
            w.apps.setString(&ns(&app.settings.excluded_apps.join("\n")));
            w.words.setString(&ns(&app.settings.exception_words.join("\n")));
        }
    }

    fn save_lists(&self) {
        let Some((apps, words)) = self
            .ivars()
            .settings
            .borrow()
            .as_ref()
            .map(|w| (w.apps.string().to_string(), w.words.string().to_string()))
        else {
            return;
        };
        let mut app = self.ivars().app.borrow_mut();
        app.settings.excluded_apps = parse_lines(&apps);
        app.settings.exception_words = parse_lines(&words).iter().map(|w| normalize(w)).collect();
        app.settings_changed();
    }

    fn build_settings_window(&self, mtm: MainThreadMarker) -> SettingsWindow {
        let (width, height) = (520.0, 550.0);
        let window = new_window(mtm, "Настройки", width, height);
        window.setDelegate(Some(ProtocolObject::from_ref(self)));
        let content = window.contentView().expect("есть содержимое окна");
        let mut y = height - 20.0;
        // Ставит элемент на всю ширину под предыдущим; возвращает его нижний край.
        let mut place = |view: &NSView, h: f64, gap: f64| -> f64 {
            y -= h;
            view.setFrame(NSRect::new(NSPoint::new(20.0, y), NSSize::new(width - 40.0, h)));
            content.addSubview(view);
            let bottom = y;
            y -= gap;
            bottom
        };

        let enabled = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &ns("Исправлять раскладку автоматически"),
                Some(self.target()),
                Some(sel!(enabledChanged:)),
                mtm,
            )
        };
        place(&enabled, 22.0, 6.0);

        let login = login_service().map(|_| {
            let b = unsafe {
                NSButton::checkboxWithTitle_target_action(
                    &ns("Запускать при входе в систему"),
                    Some(self.target()),
                    Some(sel!(loginChanged:)),
                    mtm,
                )
            };
            place(&b, 22.0, 14.0);
            b
        });

        place(&NSTextField::labelWithString(&ns("Отменить исправление или перевести слово вручную:"), mtm), 18.0, 4.0);
        let hotkey = NSPopUpButton::initWithFrame_pullsDown(mtm.alloc(), NSRect::ZERO, false);
        for h in Hotkey::ALL {
            hotkey.addItemWithTitle(&ns(h.title()));
        }
        unsafe {
            hotkey.setTarget(Some(self.target()));
            hotkey.setAction(Some(sel!(hotkeyChanged:)));
        }
        place(&hotkey, 26.0, 6.0);

        // Строка своего сочетания: надпись слева, кнопка справа.
        let combo_label = NSTextField::labelWithString(&ns(""), mtm);
        let combo_button = unsafe {
            NSButton::buttonWithTitle_target_action(&ns("Записать…"), Some(self.target()), Some(sel!(recordCombo:)), mtm)
        };
        let row = place(&combo_label, 26.0, 8.0);
        combo_label.setFrame(NSRect::new(NSPoint::new(20.0, row + 4.0), NSSize::new(width - 190.0, 18.0)));
        combo_button.setFrame(NSRect::new(NSPoint::new(width - 160.0, row), NSSize::new(140.0, 26.0)));
        content.addSubview(&combo_button);

        let backspace = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &ns("Backspace сразу после исправления отменяет его"),
                Some(self.target()),
                Some(sel!(backspaceUndoChanged:)),
                mtm,
            )
        };
        place(&backspace, 22.0, 16.0);

        let (apps_scroll, apps) = text_area(mtm);
        place(
            &NSTextField::wrappingLabelWithString(
                &ns("Приложения, где не исправлять — по одному в строке. Проще добавлять через меню: «Не исправлять в …». Терминалы и менеджеры паролей исключены всегда."),
                mtm,
            ),
            50.0,
            4.0,
        );
        place(&apps_scroll, 90.0, 14.0);

        let (words_scroll, words) = text_area(mtm);
        place(
            &NSTextField::wrappingLabelWithString(
                &ns("Слова, которые не исправлять — по одному в строке. Сюда попадают исправления, которые вы отменили."),
                mtm,
            ),
            32.0,
            4.0,
        );
        place(&words_scroll, 90.0, 12.0);

        let save = unsafe {
            NSButton::buttonWithTitle_target_action(&ns("Сохранить списки"), Some(self.target()), Some(sel!(saveLists:)), mtm)
        };
        save.setFrame(NSRect::new(NSPoint::new(width - 180.0, 16.0), NSSize::new(160.0, 30.0)));
        content.addSubview(&save);

        window.center();
        SettingsWindow { window, enabled, login, hotkey, combo_label, combo_button, backspace, apps, words }
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
            c.start_if_permitted();
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

/// Раз в секунду проверять, не выдали ли разрешение: тогда заработать
/// без перезапуска.
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
        ];
        for (name, window) in windows {
            let Some(view) = window.and_then(|w| w.contentView()) else { continue };
            let rect = view.bounds();
            let Some(bitmap) = view.bitmapImageRepForCachingDisplayInRect(rect) else { continue };
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
