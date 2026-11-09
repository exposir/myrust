//! Cross-platform desktop integrations.
//!
//! The APIs are intentionally small. They cover the Phase 1 and Phase 2
//! surface needed by typical desktop applications without exposing every
//! platform-specific knob.

/// Native text and image clipboard access.
pub mod clipboard;
/// System file and message dialogs.
pub mod dialog;
/// WebView download policy and lifecycle events.
pub mod download;
/// Single-instance guarding.
pub mod instance;
/// Native menu bar and context-menu wrappers.
pub mod menu;
/// Desktop notifications and click callbacks.
pub mod notify;
/// Power management, now-playing metadata, and login items.
pub mod power;
/// Custom WebView protocol registration.
pub mod protocol;
/// Global shortcuts.
pub mod shortcut;
/// Locale, appearance, accessibility, and proxy integration.
pub mod system;
/// System tray icon, menu, events, and balloon messages.
pub mod tray;

pub use clipboard::{Clipboard, ClipboardImage};
pub use dialog::{
    Dialog, DialogButton, DialogLevel, FileDialogOptions, FileFilter, MessageButtons,
    MessageDialogOptions,
};
pub use download::{DownloadEvent, DownloadHandler, DownloadPolicy, DownloadPolicyHandler};
pub use instance::{
    activation_socket_path_for, supports_activation_forwarding, Activation, SingleInstance,
};
pub use menu::{Menu, MenuBuilder, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
pub use notify::DesktopNotification;
pub use power::{LoginItem, NowPlaying, NowPlayingEvent, NowPlayingMetadata, Power};
pub use protocol::{CustomProtocol, ProtocolHandler, ProtocolResponse};
pub use shortcut::{GlobalShortcutManager, ShortcutEvent};
pub use system::{
    system_proxy, AppLocale, AppearancePreferences, LocaleFormatter, ProxyConfig, SystemLocale,
};
pub use tray::{
    SystemTray, SystemTrayConfig, TrayEvent, TrayIconData, TrayMenuItem, TrayMouseButton,
    TrayMouseState,
};
