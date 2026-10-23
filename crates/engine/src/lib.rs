//! Ядро Yatype: всё, что не зависит от операционной системы. Логика
//! исправления, отмена, горячие клавиши, шлюз нажатий и модель настроек.
//! Системы (macOS, Windows) подключаются через `Platform`.

pub mod double_tap;
pub mod engine;
pub mod fixer;
pub mod gate;
pub mod input;
pub mod platform;
pub mod settings;
pub mod typing;

pub use engine::Engine;
pub use input::{Input, Pass};
pub use platform::{Platform, Status};
pub use settings::{Combo, Hotkey, Settings};
pub use typing::Key;
