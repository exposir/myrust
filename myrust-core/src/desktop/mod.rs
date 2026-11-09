//! Cross-platform desktop integrations.
//!
//! The APIs are intentionally small. They cover the Phase 1 surface needed by
//! typical desktop applications without exposing every platform-specific knob.

/// System file and message dialogs.
pub mod dialog;
/// Single-instance guarding.
pub mod instance;
/// Desktop notifications and click callbacks.
pub mod notify;
/// Global shortcuts.
pub mod shortcut;
/// System tray icon, menu, events, and balloon messages.
pub mod tray;

pub use dialog::{
    Dialog, DialogButton, DialogLevel, FileDialogOptions, FileFilter, MessageButtons,
    MessageDialogOptions,
};
pub use instance::SingleInstance;
pub use notify::DesktopNotification;
pub use shortcut::{GlobalShortcutManager, ShortcutEvent};
pub use tray::{
    SystemTray, SystemTrayConfig, TrayEvent, TrayIconData, TrayMenuItem, TrayMouseButton,
    TrayMouseState,
};
