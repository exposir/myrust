//! Thin native menu wrappers built on `muda`.
//!
//! Application menu-bar support is available on macOS and Linux. Context
//! menus are available on Linux without unsafe platform calls; macOS and
//! Windows context-menu APIs require raw handles and are intentionally
//! deferred while `myrust-core` forbids unsafe code.

use std::sync::{
    mpsc::{channel, Receiver, Sender},
    LazyLock, Mutex, Once,
};

#[cfg(not(target_os = "linux"))]
use anyhow::bail;
use anyhow::{Context, Result};
use tao::window::Window;

/// Event emitted when the user selects a custom menu item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEvent {
    /// Application-defined menu item id.
    pub id: String,
}

static MENU_SUBSCRIBERS: LazyLock<Mutex<Vec<Sender<MenuEvent>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static INSTALL_MENU_DISPATCHER: Once = Once::new();

/// Subscribes to process-wide native menu item events.
///
/// The returned channel receives events for application menus, context
/// menus, and tray menus. The caller should keep receiving until it is done.
pub fn subscribe() -> Receiver<MenuEvent> {
    ensure_menu_event_dispatcher();

    let (sender, receiver) = channel();
    MENU_SUBSCRIBERS
        .lock()
        .expect("menu subscriber registry poisoned")
        .push(sender);
    receiver
}

/// Installs the single native menu-event dispatcher used by apps and trays.
pub(crate) fn ensure_menu_event_dispatcher() {
    INSTALL_MENU_DISPATCHER.call_once(|| {
        muda::MenuEvent::set_event_handler(Some(|event: muda::MenuEvent| {
            let event = MenuEvent {
                id: event.id().0.clone(),
            };

            let mut subscribers = MENU_SUBSCRIBERS
                .lock()
                .expect("menu subscriber registry poisoned");
            subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
            drop(subscribers);

            super::tray::route_tray_menu_event(&event.id);
        }));
    });
}

/// Native root menu.
#[derive(Clone)]
pub struct Menu {
    inner: muda::Menu,
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu {
    /// Creates an empty root menu.
    pub fn new() -> Self {
        Self {
            inner: muda::Menu::new(),
        }
    }

    /// Starts a fluent [`MenuBuilder`].
    pub fn builder() -> MenuBuilder {
        MenuBuilder::new()
    }

    /// Appends a custom clickable item.
    pub fn append_item(&self, item: &MenuItem) -> Result<()> {
        self.inner
            .append(&item.inner)
            .context("failed to append menu item")
    }

    /// Appends a submenu.
    pub fn append_submenu(&self, submenu: &Submenu) -> Result<()> {
        self.inner
            .append(&submenu.inner)
            .context("failed to append submenu")
    }

    /// Appends a platform-defined item such as Copy, Quit, or a separator.
    pub fn append_predefined(&self, item: &PredefinedMenuItem) -> Result<()> {
        self.inner
            .append(&item.inner)
            .context("failed to append predefined menu item")
    }

    /// Attaches this menu to a native application window.
    ///
    /// On macOS this installs the process-wide application menu. On Linux it
    /// installs a per-window GTK menu bar. Windows menu-bar attachment requires
    /// a raw HWND and is not available while unsafe code is forbidden.
    pub fn attach_to_window(&self, window: &Window) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            use tao::platform::unix::WindowExtUnix;

            self.inner
                .init_for_gtk_window(window.gtk_window(), window.default_vbox())
                .context("failed to attach menu to GTK window")
        }

        #[cfg(target_os = "macos")]
        {
            let _ = window;
            self.inner.init_for_nsapp();
            Ok(())
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = window;
            bail!(
                "window menu attachment is unavailable on this platform without an unsafe raw-window-handle API"
            )
        }
    }

    /// Shows this menu as a context menu at a logical window position.
    ///
    /// The selected item is reported through [`subscribe`]. Returns whether
    /// the menu was dismissed after selecting an item.
    pub fn show_context_menu(&self, window: &Window, position: (f64, f64)) -> Result<bool> {
        #[cfg(target_os = "linux")]
        {
            use gtk::prelude::Cast;
            use muda::ContextMenu;
            use tao::platform::unix::WindowExtUnix;

            let position = Some(muda::dpi::Position::Logical(
                muda::dpi::LogicalPosition::new(position.0, position.1),
            ));
            let gtk_window: &gtk::Window = window.gtk_window().upcast_ref();
            Ok(self
                .inner
                .show_context_menu_for_gtk_window(gtk_window, position))
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (window, position);
            bail!(
                "context menus are available on Linux during Phase 2; this platform requires an unsafe raw-window-handle API"
            )
        }
    }
}

/// Custom clickable menu item.
#[derive(Clone)]
pub struct MenuItem {
    inner: muda::MenuItem,
}

impl MenuItem {
    /// Creates a menu item with a stable application id.
    pub fn new(id: impl Into<String>, label: impl Into<String>, enabled: bool) -> Self {
        Self {
            inner: muda::MenuItem::with_id(id.into(), label.into(), enabled, None),
        }
    }

    /// Returns the application-defined item id.
    pub fn id(&self) -> String {
        self.inner.id().0.clone()
    }

    /// Enables or disables the item.
    pub fn set_enabled(&self, enabled: bool) {
        self.inner.set_enabled(enabled);
    }
}

/// Native submenu.
#[derive(Clone)]
pub struct Submenu {
    inner: muda::Submenu,
}

impl Submenu {
    /// Creates a submenu with a label.
    pub fn new(label: impl Into<String>, enabled: bool) -> Self {
        Self {
            inner: muda::Submenu::new(label.into(), enabled),
        }
    }

    /// Appends a custom clickable item.
    pub fn append_item(&self, item: &MenuItem) -> Result<()> {
        self.inner
            .append(&item.inner)
            .context("failed to append submenu item")
    }

    /// Appends a nested submenu.
    pub fn append_submenu(&self, submenu: &Submenu) -> Result<()> {
        self.inner
            .append(&submenu.inner)
            .context("failed to append nested submenu")
    }

    /// Appends a platform-defined item.
    pub fn append_predefined(&self, item: &PredefinedMenuItem) -> Result<()> {
        self.inner
            .append(&item.inner)
            .context("failed to append predefined submenu item")
    }
}

#[derive(Debug, Clone, Copy)]
enum PredefinedKind {
    Separator,
    Copy,
    Cut,
    Paste,
    SelectAll,
    Quit,
    About,
}

/// Platform-defined menu item.
#[derive(Clone)]
pub struct PredefinedMenuItem {
    inner: muda::PredefinedMenuItem,
    kind: PredefinedKind,
}

impl PredefinedMenuItem {
    /// Creates a separator.
    pub fn separator() -> Self {
        Self {
            inner: muda::PredefinedMenuItem::separator(),
            kind: PredefinedKind::Separator,
        }
    }

    /// Creates the native Copy item.
    pub fn copy() -> Self {
        Self {
            inner: muda::PredefinedMenuItem::copy(None),
            kind: PredefinedKind::Copy,
        }
    }

    /// Creates the native Cut item.
    pub fn cut() -> Self {
        Self {
            inner: muda::PredefinedMenuItem::cut(None),
            kind: PredefinedKind::Cut,
        }
    }

    /// Creates the native Paste item.
    pub fn paste() -> Self {
        Self {
            inner: muda::PredefinedMenuItem::paste(None),
            kind: PredefinedKind::Paste,
        }
    }

    /// Creates the native Select All item.
    pub fn select_all() -> Self {
        Self {
            inner: muda::PredefinedMenuItem::select_all(None),
            kind: PredefinedKind::SelectAll,
        }
    }

    /// Creates the native Quit item.
    pub fn quit() -> Self {
        Self {
            inner: muda::PredefinedMenuItem::quit(None),
            kind: PredefinedKind::Quit,
        }
    }

    /// Creates the native About item.
    pub fn about() -> Self {
        Self {
            inner: muda::PredefinedMenuItem::about(None, None),
            kind: PredefinedKind::About,
        }
    }
}

/// Fluent builder for a native root menu.
#[derive(Default)]
pub struct MenuBuilder {
    children: Vec<MenuNode>,
}

enum MenuNode {
    Item {
        id: String,
        label: String,
        enabled: bool,
    },
    Submenu {
        label: String,
        enabled: bool,
        children: Vec<MenuNode>,
    },
    Predefined(PredefinedKind),
}

impl MenuBuilder {
    /// Creates an empty menu builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an enabled custom item.
    pub fn add_item(self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.add_item_with_enabled(id, label, true)
    }

    /// Adds a custom item with an explicit enabled state.
    pub fn add_item_with_enabled(
        mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        enabled: bool,
    ) -> Self {
        self.children.push(MenuNode::Item {
            id: id.into(),
            label: label.into(),
            enabled,
        });
        self
    }

    /// Adds a submenu populated by `build`.
    pub fn add_submenu<F>(mut self, label: impl Into<String>, build: F) -> Self
    where
        F: FnOnce(MenuBuilder) -> MenuBuilder,
    {
        let builder = build(MenuBuilder::new());
        self.children.push(MenuNode::Submenu {
            label: label.into(),
            enabled: true,
            children: builder.children,
        });
        self
    }

    /// Adds a separator.
    pub fn add_separator(mut self) -> Self {
        self.children
            .push(MenuNode::Predefined(PredefinedKind::Separator));
        self
    }

    /// Adds a platform-defined item.
    pub fn add_predefined(mut self, item: PredefinedMenuItem) -> Self {
        self.children.push(MenuNode::Predefined(item.kind));
        self
    }

    /// Builds the native menu.
    pub fn build(self) -> Result<Menu> {
        let inner = muda::Menu::new();
        append_children(&inner, self.children)?;
        Ok(Menu { inner })
    }
}

fn append_children(menu: &muda::Menu, children: Vec<MenuNode>) -> Result<()> {
    for child in children {
        match child {
            MenuNode::Item { id, label, enabled } => {
                let item = muda::MenuItem::with_id(id, label, enabled, None);
                menu.append(&item).context("failed to append menu item")?;
            }
            MenuNode::Submenu {
                label,
                enabled,
                children,
            } => {
                let submenu = muda::Submenu::new(label, enabled);
                append_submenu_children(&submenu, children)?;
                menu.append(&submenu).context("failed to append submenu")?;
            }
            MenuNode::Predefined(kind) => {
                let item = build_predefined(kind);
                menu.append(&item)
                    .context("failed to append predefined item")?;
            }
        }
    }
    Ok(())
}

fn append_submenu_children(menu: &muda::Submenu, children: Vec<MenuNode>) -> Result<()> {
    for child in children {
        match child {
            MenuNode::Item { id, label, enabled } => {
                let item = muda::MenuItem::with_id(id, label, enabled, None);
                menu.append(&item)
                    .context("failed to append submenu item")?;
            }
            MenuNode::Submenu {
                label,
                enabled,
                children,
            } => {
                let submenu = muda::Submenu::new(label, enabled);
                append_submenu_children(&submenu, children)?;
                menu.append(&submenu)
                    .context("failed to append nested submenu")?;
            }
            MenuNode::Predefined(kind) => {
                let item = build_predefined(kind);
                menu.append(&item)
                    .context("failed to append predefined submenu item")?;
            }
        }
    }
    Ok(())
}

fn build_predefined(kind: PredefinedKind) -> muda::PredefinedMenuItem {
    match kind {
        PredefinedKind::Separator => muda::PredefinedMenuItem::separator(),
        PredefinedKind::Copy => muda::PredefinedMenuItem::copy(None),
        PredefinedKind::Cut => muda::PredefinedMenuItem::cut(None),
        PredefinedKind::Paste => muda::PredefinedMenuItem::paste(None),
        PredefinedKind::SelectAll => muda::PredefinedMenuItem::select_all(None),
        PredefinedKind::Quit => muda::PredefinedMenuItem::quit(None),
        PredefinedKind::About => muda::PredefinedMenuItem::about(None, None),
    }
}
