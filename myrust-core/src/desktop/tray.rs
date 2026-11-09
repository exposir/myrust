//! System tray icon and menu support.
//!
//! Linux uses the tray-icon AppIndicator backend. AppIndicator menus work on
//! Linux, while direct tray click events depend on the platform backend.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, Once, RwLock},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use tray_icon::{
    menu::{Menu, MenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent, TrayIconId,
};

use super::notify::DesktopNotification;

type TrayCallback = Arc<dyn Fn(TrayEvent) + Send + Sync + 'static>;
type CallbackSlot = Arc<RwLock<Option<TrayCallback>>>;

static TRAY_HANDLERS: LazyLock<Mutex<HashMap<String, CallbackSlot>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static INSTALL_HANDLERS: Once = Once::new();

/// Mouse button associated with a tray click.
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum TrayMouseButton {
    /// Primary button.
    Left = 0,
    /// Secondary button.
    Right = 1,
    /// Middle button.
    Middle = 2,
}

/// Press state associated with a tray click.
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum TrayMouseState {
    /// The button was released.
    Up = 0,
    /// The button was pressed.
    Down = 1,
}

/// Event emitted by a system tray.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TrayEvent {
    /// A tray icon click.
    Click {
        /// Mouse button that triggered the event.
        button: TrayMouseButton,
        /// Press or release state.
        state: TrayMouseState,
    },
    /// A tray icon double click.
    DoubleClick {
        /// Mouse button that triggered the event.
        button: TrayMouseButton,
    },
    /// A native tray menu item was selected.
    MenuItem {
        /// Application-defined menu item id.
        id: String,
    },
}

/// RGBA pixels used to create a tray icon.
#[derive(Debug, Clone)]
pub struct TrayIconData {
    /// RGBA bytes in row-major order.
    pub rgba: Vec<u8>,
    /// Icon width in pixels.
    pub width: u32,
    /// Icon height in pixels.
    pub height: u32,
}

impl TrayIconData {
    /// Creates RGBA icon data.
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            rgba,
            width,
            height,
        }
    }

    /// Returns a small cyan-on-dark default icon used by the dashboard demo.
    pub fn default_dashboard_icon() -> Self {
        const SIZE: u32 = 32;
        let mut rgba = vec![0_u8; (SIZE * SIZE * 4) as usize];

        for y in 0..SIZE {
            for x in 0..SIZE {
                let dx = x as f32 - 15.5;
                let dy = y as f32 - 15.5;
                let distance = (dx * dx + dy * dy).sqrt();
                let index = ((y * SIZE + x) * 4) as usize;

                if distance <= 12.0 {
                    rgba[index] = 34;
                    rgba[index + 1] = 211;
                    rgba[index + 2] = 238;
                    rgba[index + 3] = 255;
                }
                if distance <= 5.0 {
                    rgba[index] = 11;
                    rgba[index + 1] = 13;
                    rgba[index + 2] = 18;
                    rgba[index + 3] = 255;
                }
            }
        }

        Self::new(rgba, SIZE, SIZE)
    }

    fn to_icon(&self) -> Result<Icon> {
        Icon::from_rgba(self.rgba.clone(), self.width, self.height)
            .context("invalid tray icon RGBA data")
    }
}

/// One native tray menu item.
#[derive(Debug, Clone)]
pub struct TrayMenuItem {
    /// Application-defined item id, reported in [`TrayEvent::MenuItem`].
    pub id: String,
    /// Visible menu label.
    pub label: String,
    /// Whether the item accepts clicks.
    pub enabled: bool,
}

impl TrayMenuItem {
    /// Creates an enabled menu item.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
        }
    }
}

/// Configuration for a system tray icon.
#[derive(Debug, Clone)]
pub struct SystemTrayConfig {
    /// Stable application-defined tray id.
    pub id: String,
    /// Optional hover tooltip.
    pub tooltip: Option<String>,
    /// Optional icon; a built-in icon is used when omitted.
    pub icon: Option<TrayIconData>,
    /// Native context menu items.
    pub menu: Vec<TrayMenuItem>,
}

impl Default for SystemTrayConfig {
    fn default() -> Self {
        Self {
            id: "myrust".to_string(),
            tooltip: Some("myrust".to_string()),
            icon: None,
            menu: Vec::new(),
        }
    }
}

/// Native system tray icon, menu, and event dispatcher.
pub struct SystemTray {
    id: String,
    icon: TrayIcon,
    _menu_items: Vec<MenuItem>,
    callback: CallbackSlot,
}

impl SystemTray {
    /// Creates a native tray icon.
    ///
    /// Call this from the application's main thread after the native toolkit
    /// has been initialized.
    pub fn new(config: SystemTrayConfig) -> Result<Self> {
        super::menu::ensure_menu_event_dispatcher();
        install_global_handlers();

        let tray_id = TrayIconId::new(&config.id);
        let callback = Arc::new(RwLock::new(None));
        TRAY_HANDLERS
            .lock()
            .expect("tray handler registry poisoned")
            .insert(config.id.clone(), callback.clone());

        let menu = Menu::new();
        let mut menu_items = Vec::with_capacity(config.menu.len());
        for item in &config.menu {
            let menu_id = format!("{}/{}", config.id, item.id);
            let menu_item = MenuItem::with_id(menu_id, &item.label, item.enabled, None);
            menu.append(&menu_item)
                .with_context(|| format!("failed to append tray menu item `{}`", item.id))?;
            menu_items.push(menu_item);
        }

        let icon_data = config
            .icon
            .unwrap_or_else(TrayIconData::default_dashboard_icon);
        let mut builder = TrayIconBuilder::new()
            .with_id(tray_id)
            .with_icon(icon_data.to_icon()?)
            .with_menu(Box::new(menu));

        if let Some(tooltip) = config.tooltip {
            builder = builder.with_tooltip(tooltip);
        }

        let icon = builder
            .build()
            .context("failed to create system tray icon")?;

        Ok(Self {
            id: config.id,
            icon,
            _menu_items: menu_items,
            callback,
        })
    }

    /// Returns the application-defined tray id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Registers a callback for tray clicks and menu selections.
    pub fn on_event<F>(&self, callback: F)
    where
        F: Fn(TrayEvent) + Send + Sync + 'static,
    {
        *self.callback.write().expect("tray callback lock poisoned") = Some(Arc::new(callback));
    }

    /// Replaces the tray icon.
    pub fn set_icon(&self, icon: TrayIconData) -> Result<()> {
        self.icon
            .set_icon(Some(icon.to_icon()?))
            .context("failed to replace tray icon")
    }

    /// Shows a desktop balloon notification associated with the tray.
    pub fn show_balloon(&self, title: impl Into<String>, body: impl Into<String>) -> Result<()> {
        DesktopNotification::new(title, body).send()
    }
}

impl Drop for SystemTray {
    fn drop(&mut self) {
        TRAY_HANDLERS
            .lock()
            .expect("tray handler registry poisoned")
            .remove(&self.id);
    }
}

fn install_global_handlers() {
    INSTALL_HANDLERS.call_once(|| {
        TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
            let tray_id = event.id().0.clone();
            let Some(event) = convert_tray_event(event) else {
                return;
            };
            dispatch_tray_event(&tray_id, event);
        }));
    });
}

/// Routes a prefixed tray menu id to the matching tray callback.
pub(crate) fn route_tray_menu_event(menu_id: &str) {
    let Some((tray_id, item_id)) = menu_id.split_once('/') else {
        return;
    };
    dispatch_tray_event(
        tray_id,
        TrayEvent::MenuItem {
            id: item_id.to_string(),
        },
    );
}

fn convert_tray_event(event: TrayIconEvent) -> Option<TrayEvent> {
    match event {
        TrayIconEvent::Click {
            button,
            button_state,
            ..
        } => Some(TrayEvent::Click {
            button: convert_mouse_button(button),
            state: convert_mouse_state(button_state),
        }),
        TrayIconEvent::DoubleClick { button, .. } => Some(TrayEvent::DoubleClick {
            button: convert_mouse_button(button),
        }),
        _ => None,
    }
}

fn convert_mouse_button(button: MouseButton) -> TrayMouseButton {
    match button {
        MouseButton::Left => TrayMouseButton::Left,
        MouseButton::Right => TrayMouseButton::Right,
        MouseButton::Middle => TrayMouseButton::Middle,
    }
}

fn convert_mouse_state(state: MouseButtonState) -> TrayMouseState {
    match state {
        MouseButtonState::Up => TrayMouseState::Up,
        MouseButtonState::Down => TrayMouseState::Down,
    }
}

fn dispatch_tray_event(tray_id: &str, event: TrayEvent) {
    let slot = TRAY_HANDLERS
        .lock()
        .expect("tray handler registry poisoned")
        .get(tray_id)
        .cloned();

    if let Some(slot) = slot {
        let callback = slot.read().expect("tray callback lock poisoned").clone();
        if let Some(callback) = callback {
            callback(event);
        }
    }
}
