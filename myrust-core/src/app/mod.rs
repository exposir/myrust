//! Application lifecycle and multi-window management.
//!
//! One [`App`] instance owns the native event loop, all windows, the typed IPC
//! router, and optional development reload watchers.

/// Deep-link scheme registry and URL delivery.
pub mod deep_link;
/// Public event-driven filesystem watcher.
pub mod file_watcher;
/// Recently opened documents and jump-list interfaces.
pub mod recent_documents;
/// Window geometry persistence.
pub mod window_state;

pub use deep_link::DeepLinkRegistry;
pub use file_watcher::{
    FileEvent, FileEventDetail, FileEventKind, FileWatcher, WatchHandle, WatchOptions,
};
pub use recent_documents::{JumpList, RecentDocument, RecentDocuments, UnsupportedJumpList};
pub use window_state::{JsonFileStore, WindowState, WindowStateStore};

use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    window::{Fullscreen, Window, WindowBuilder, WindowId as TaoWindowId},
};

use crate::{
    desktop::menu::{self, Menu, MenuEvent},
    ipc::{self, IpcReply, IpcRouter},
    webview::{FileDropEvent, IpcMessageHandler, PageHandle, WebViewConfig, WebViewHandle},
};

static APP_CREATED: AtomicBool = AtomicBool::new(false);
const FIT_CONTENT_CHANNEL: &str = "__myrust_fit_content";
const FIT_CONTENT_SCRIPT: &str = r#"
(() => {
  const root = document.documentElement;
  const body = document.body;
  const width = Math.ceil(Math.max(
    root ? root.scrollWidth : 0,
    body ? body.scrollWidth : 0
  ));
  const height = Math.ceil(Math.max(
    root ? root.scrollHeight : 0,
    body ? body.scrollHeight : 0
  ));
  window.ipc.postMessage(JSON.stringify({
    id: 0,
    channel: "__myrust_fit_content",
    payload: { width, height }
  }));
})();
"#;

/// Stable application-managed window identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowId(
    /// Raw application window value.
    pub u64,
);

/// Platform window vibrancy style.
///
/// Tao does not expose native vibrancy in this build, so these values are
/// recorded in configuration but currently applied as no-ops on every
/// platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vibrancy {
    /// Frosted blur behind the window.
    Blur,
    /// Desktop material behind the window.
    UnderWindowBackground,
    /// Sidebar material.
    Sidebar,
    /// Popover material.
    Popover,
}

/// Title-bar button visibility preferences.
///
/// macOS can hide all traffic-light buttons together. Selective and
/// non-macOS title-button control are currently no-ops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleButtons {
    /// Show the close button.
    pub close: bool,
    /// Show the minimize button.
    pub minimize: bool,
    /// Show the zoom/maximize button.
    pub maximize: bool,
}

impl Default for TitleButtons {
    fn default() -> Self {
        Self {
            close: true,
            minimize: true,
            maximize: true,
        }
    }
}

/// Logical rectangle used for input-region hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Logical x offset from the window top-left corner.
    pub x: i32,
    /// Logical y offset from the window top-left corner.
    pub y: i32,
    /// Logical rectangle width.
    pub width: i32,
    /// Logical rectangle height.
    pub height: i32,
}

/// Initial configuration for a native application window.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    /// Native window title.
    pub title: String,
    /// Logical width in density-independent pixels.
    pub width: f64,
    /// Logical height in density-independent pixels.
    pub height: f64,
    /// Optional logical minimum size.
    pub min_size: Option<(f64, f64)>,
    /// Optional logical screen position.
    pub position: Option<(f64, f64)>,
    /// Starts the window full screen.
    pub fullscreen: bool,
    /// Shows native title bar and borders.
    pub decorations: bool,
    /// Allows the user to resize the window.
    pub resizable: bool,
    /// Shows the window after creation.
    pub visible: bool,
    /// Stable key used by window-state persistence.
    pub window_key: Option<String>,
    /// Optional platform vibrancy style.
    pub vibrancy: Option<Vibrancy>,
    /// Requests a transparent native window and WebView background.
    pub transparent: bool,
    /// Keeps the window above normal windows.
    pub always_on_top: bool,
    /// Requests panel-style behavior.
    ///
    /// Tao does not expose a portable panel window level in this build, so the
    /// value is currently recorded but applied as a no-op.
    pub panel: bool,
    /// Hides the window from the taskbar where supported.
    pub skip_taskbar: bool,
    /// Optional title-bar button visibility preferences.
    pub title_buttons: Option<TitleButtons>,
    /// Optional native WebView configuration.
    pub webview: Option<WebViewConfig>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "myrust".to_string(),
            width: 1024.0,
            height: 720.0,
            min_size: Some((480.0, 320.0)),
            position: None,
            fullscreen: false,
            decorations: true,
            resizable: true,
            visible: true,
            window_key: None,
            vibrancy: None,
            transparent: false,
            always_on_top: false,
            panel: false,
            skip_taskbar: false,
            title_buttons: None,
            webview: None,
        }
    }
}

/// Snapshot of an open application window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Stable application window id.
    pub id: WindowId,
    /// Current native window title.
    pub title: String,
    /// Logical width.
    pub width: f64,
    /// Logical height.
    pub height: f64,
    /// Configured fullscreen state.
    pub fullscreen: bool,
    /// Whether native decorations are enabled.
    pub decorations: bool,
    /// Whether the window owns a WebView.
    pub has_webview: bool,
}

/// Application lifecycle and configuration.
pub struct App {
    name: String,
    ipc_router: Arc<IpcRouter>,
    on_ready: Vec<LifecycleCallback>,
    on_activate: Vec<ActivateCallback>,
    on_all_closed: Vec<LifecycleCallback>,
    on_menu_event: Vec<MenuEventCallback>,
    on_file_drop: Vec<FileDropCallback>,
    on_open_url: Vec<OpenUrlCallback>,
    app_menu: Option<Menu>,
    window_state_store: Option<Arc<dyn WindowStateStore>>,
    deep_links: DeepLinkRegistry,
    exit_when_last_window_closes: bool,
    watch_paths: Vec<PathBuf>,
}

type LifecycleCallback = Box<dyn for<'a> FnMut(&mut AppContext<'a>) -> Result<()> + 'static>;
type ActivateCallback = Box<dyn for<'a> FnMut(&mut AppContext<'a>, bool) -> Result<()> + 'static>;
type MenuEventCallback =
    Box<dyn for<'a> FnMut(&mut AppContext<'a>, MenuEvent) -> Result<()> + 'static>;
type FileDropCallback =
    Box<dyn for<'a> FnMut(&mut AppContext<'a>, WindowId, FileDropEvent) -> Result<()> + 'static>;
type OpenUrlCallback = Box<dyn for<'a> FnMut(&mut AppContext<'a>, String) -> Result<()> + 'static>;

impl App {
    /// Creates the process singleton application.
    ///
    /// A second call in the same process returns an error.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        if APP_CREATED.swap(true, Ordering::SeqCst) {
            bail!("myrust App is already created in this process");
        }

        Ok(Self {
            name: name.into(),
            ipc_router: Arc::new(IpcRouter::new()),
            on_ready: Vec::new(),
            on_activate: Vec::new(),
            on_all_closed: Vec::new(),
            on_menu_event: Vec::new(),
            on_file_drop: Vec::new(),
            on_open_url: Vec::new(),
            app_menu: None,
            window_state_store: None,
            deep_links: DeepLinkRegistry::new(),
            exit_when_last_window_closes: true,
            watch_paths: Vec::new(),
        })
    }

    /// Returns the application IPC router.
    pub fn ipc_router(&self) -> &IpcRouter {
        &self.ipc_router
    }

    /// Registers a callback invoked once before the native event loop starts.
    pub fn on_ready<F>(&mut self, callback: F) -> &mut Self
    where
        F: for<'a> FnMut(&mut AppContext<'a>) -> Result<()> + 'static,
    {
        self.on_ready.push(Box::new(callback));
        self
    }

    /// Registers a callback invoked when the app receives activation or focus.
    pub fn on_activate<F>(&mut self, callback: F) -> &mut Self
    where
        F: for<'a> FnMut(&mut AppContext<'a>, bool) -> Result<()> + 'static,
    {
        self.on_activate.push(Box::new(callback));
        self
    }

    /// Registers a callback invoked when the final window closes.
    pub fn on_all_closed<F>(&mut self, callback: F) -> &mut Self
    where
        F: for<'a> FnMut(&mut AppContext<'a>) -> Result<()> + 'static,
    {
        self.on_all_closed.push(Box::new(callback));
        self
    }

    /// Sets the application menu attached to every managed window.
    ///
    /// On macOS this becomes the global application menu bar. On Linux it is
    /// installed as a GTK menu bar in each window.
    pub fn set_app_menu(&mut self, menu: Menu) -> &mut Self {
        self.app_menu = Some(menu);
        self
    }

    /// Enables automatic window-state restore and save.
    ///
    /// Only windows with [`WindowConfig::window_key`] participate. State is
    /// written before a window is removed from the native window map when it
    /// closes through either Tao or [`AppContext::close_window`].
    pub fn restore_window_state(&mut self, store: Arc<dyn WindowStateStore>) -> &mut Self {
        self.window_state_store = Some(store);
        self
    }

    /// Registers a callback invoked when a native menu item is selected.
    pub fn on_menu_event<F>(&mut self, callback: F) -> &mut Self
    where
        F: for<'a> FnMut(&mut AppContext<'a>, MenuEvent) -> Result<()> + 'static,
    {
        self.on_menu_event.push(Box::new(callback));
        self
    }

    /// Registers a callback invoked for native WebView file drag-and-drop events.
    pub fn on_file_drop<F>(&mut self, callback: F) -> &mut Self
    where
        F: for<'a> FnMut(&mut AppContext<'a>, WindowId, FileDropEvent) -> Result<()> + 'static,
    {
        self.on_file_drop.push(Box::new(callback));
        self
    }

    /// Registers a deep-link URL scheme for in-process validation.
    ///
    /// OS-level scheme registration requires macOS bundle metadata or Windows
    /// installer registration and is deferred to Phase 4.
    pub fn register_deep_link(&mut self, scheme: impl Into<String>) -> Result<&mut Self> {
        self.deep_links.register(scheme)?;
        Ok(self)
    }

    /// Registers a callback for validated deep-link URLs.
    pub fn on_open_url<F>(&mut self, callback: F) -> &mut Self
    where
        F: for<'a> FnMut(&mut AppContext<'a>, String) -> Result<()> + 'static,
    {
        self.on_open_url.push(Box::new(callback));
        self
    }

    /// Controls whether the app exits after its final window closes.
    pub fn exit_when_last_window_closes(&mut self, enabled: bool) -> &mut Self {
        self.exit_when_last_window_closes = enabled;
        self
    }

    /// Watches a file or directory and reloads all WebViews after changes.
    ///
    /// The watcher uses the platform-native `notify` backend and is intended
    /// for local development.
    pub fn watch_path(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.watch_paths.push(path.into());
        self
    }

    /// Writes generated TypeScript IPC declarations.
    pub fn generate_typescript(&self, path: impl AsRef<Path>) -> Result<()> {
        ipc::gen::TypeScriptGenerator::write(&self.ipc_router, &self.name, path)
    }

    /// Starts the native event loop and blocks until the application exits.
    ///
    /// If `MYRUST_GENERATE_TYPES` points to a file, declarations are generated
    /// and the method returns without opening a window.
    pub fn run(mut self) -> Result<()> {
        if let Some(path) = env::var_os("MYRUST_GENERATE_TYPES") {
            self.generate_typescript(path)?;
            return Ok(());
        }

        let app_name = self.name.clone();
        let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
        let proxy = event_loop.create_proxy();
        let ipc_router = self.ipc_router.clone();
        let app_menu = self.app_menu.take();
        let window_state_store = self.window_state_store.take();
        let deep_links = self.deep_links.clone();
        let file_drop_enabled = !self.on_file_drop.is_empty();
        let mut windows = WindowManager::new(
            ipc_router,
            proxy.clone(),
            app_menu,
            file_drop_enabled,
            app_name.clone(),
            window_state_store,
            deep_links,
        );

        let watch_paths = std::mem::take(&mut self.watch_paths);
        let mut context = AppContext::new(&event_loop, &mut windows, &proxy);
        for callback in &mut self.on_ready {
            callback(&mut context).context("on_ready callback failed")?;
        }

        if windows.is_empty() && self.exit_when_last_window_closes {
            bail!("on_ready created no windows");
        }

        let mut on_activate = self.on_activate;
        let mut on_all_closed = self.on_all_closed;
        let mut on_menu_event = self.on_menu_event;
        let mut on_file_drop = self.on_file_drop;
        let mut on_open_url = self.on_open_url;
        let exit_when_last_window_closes = self.exit_when_last_window_closes;
        let watchers = watch_paths
            .into_iter()
            .filter_map(|path| match ReloadWatcher::spawn(path, proxy.clone()) {
                Ok(watcher) => Some(watcher),
                Err(error) => {
                    tracing::warn!(%error, "failed to start hot-reload watcher");
                    None
                }
            })
            .collect::<Vec<_>>();
        let menu_forwarder = MenuEventForwarder::spawn(proxy.clone())
            .context("failed to start menu event forwarder")?;

        tracing::info!(app = %app_name, "starting native event loop");
        event_loop.run(move |event, target, control_flow| {
            let _runtime_support = (&watchers, &menu_forwarder);
            *control_flow = ControlFlow::Wait;

            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    window_id,
                    ..
                } => {
                    if windows.close_by_tao_id(window_id) {
                        handle_last_window_closed(
                            target,
                            &mut windows,
                            &proxy,
                            &mut on_all_closed,
                            exit_when_last_window_closes,
                            control_flow,
                        );
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::Focused(true),
                    ..
                } => {
                    let visible = !windows.is_empty();
                    let mut context = AppContext::new(target, &mut windows, &proxy);
                    for callback in &mut on_activate {
                        if let Err(error) = callback(&mut context, visible) {
                            tracing::error!(%error, "on_activate callback failed");
                        }
                    }
                }
                Event::Reopen {
                    has_visible_windows,
                    ..
                } => {
                    let mut context = AppContext::new(target, &mut windows, &proxy);
                    for callback in &mut on_activate {
                        if let Err(error) = callback(&mut context, has_visible_windows) {
                            tracing::error!(%error, "on_activate callback failed");
                        }
                    }
                }
                Event::UserEvent(UserEvent::CloseWindow(window_id)) => {
                    if windows.close(window_id) {
                        handle_last_window_closed(
                            target,
                            &mut windows,
                            &proxy,
                            &mut on_all_closed,
                            exit_when_last_window_closes,
                            control_flow,
                        );
                    }
                }
                Event::UserEvent(UserEvent::IpcReply { window_id, reply }) => {
                    windows.send_reply(window_id, &reply);
                }
                Event::UserEvent(UserEvent::Menu(event)) => {
                    let mut context = AppContext::new(target, &mut windows, &proxy);
                    for callback in &mut on_menu_event {
                        if let Err(error) = callback(&mut context, event.clone()) {
                            tracing::error!(%error, id = %event.id, "menu callback failed");
                        }
                    }
                }
                Event::UserEvent(UserEvent::FileDrop { window_id, event }) => {
                    let mut context = AppContext::new(target, &mut windows, &proxy);
                    for callback in &mut on_file_drop {
                        if let Err(error) = callback(&mut context, window_id, event.clone()) {
                            tracing::error!(%error, ?window_id, "file drop callback failed");
                        }
                    }
                }
                Event::UserEvent(UserEvent::OpenUrl(url)) => {
                    let mut context = AppContext::new(target, &mut windows, &proxy);
                    for callback in &mut on_open_url {
                        if let Err(error) = callback(&mut context, url.clone()) {
                            tracing::error!(%error, %url, "open-url callback failed");
                        }
                    }
                }
                Event::UserEvent(UserEvent::FitContent {
                    window_id,
                    width,
                    height,
                }) => {
                    if let Some(window) = windows.get_mut(window_id) {
                        window
                            .window
                            .set_inner_size(LogicalSize::new(width, height));
                    } else {
                        tracing::warn!(?window_id, "fit-content target window is closed");
                    }
                }
                Event::UserEvent(UserEvent::ReloadAll) => {
                    windows.reload_all();
                }
                Event::UserEvent(UserEvent::Quit) => {
                    *control_flow = ControlFlow::Exit;
                }
                _ => {}
            }
        })
    }
}

/// Callback context with native window and WebView controls.
pub struct AppContext<'a> {
    target: &'a EventLoopWindowTarget<UserEvent>,
    windows: &'a mut WindowManager,
    proxy: &'a EventLoopProxy<UserEvent>,
}

impl<'a> AppContext<'a> {
    fn new(
        target: &'a EventLoopWindowTarget<UserEvent>,
        windows: &'a mut WindowManager,
        proxy: &'a EventLoopProxy<UserEvent>,
    ) -> Self {
        Self {
            target,
            windows,
            proxy,
        }
    }

    /// Creates a native window and optional WebView.
    pub fn create_window(&mut self, config: WindowConfig) -> Result<WindowId> {
        self.windows.create_window(self.target, config)
    }

    /// Requests that a window closes.
    pub fn close_window(&mut self, id: WindowId) -> Result<()> {
        if !self.windows.contains(id) {
            bail!("window {:?} is not open", id);
        }
        self.proxy
            .send_event(UserEvent::CloseWindow(id))
            .map_err(|_| anyhow!("native event loop is no longer available"))
    }

    /// Returns a snapshot of every open window.
    pub fn windows(&self) -> Vec<WindowInfo> {
        self.windows.list()
    }

    /// Updates a window title.
    pub fn set_window_title(&mut self, id: WindowId, title: impl Into<String>) -> Result<()> {
        let window = self
            .windows
            .get_mut(id)
            .ok_or_else(|| anyhow!("window {:?} is not open", id))?;
        window.title = title.into();
        window.window.set_title(&window.title);
        Ok(())
    }

    /// Attaches a native menu bar to one window.
    pub fn set_menu(&mut self, id: WindowId, menu: Menu) -> Result<()> {
        let window = self
            .windows
            .get_mut(id)
            .ok_or_else(|| anyhow!("window {:?} is not open", id))?;
        menu.attach_to_window(&window.window)
            .context("failed to attach window menu")?;
        window.menu = Some(menu);
        Ok(())
    }

    /// Shows a native context menu at a logical window position.
    ///
    /// Returns whether an item was selected before the menu closed.
    pub fn show_context_menu(
        &mut self,
        id: WindowId,
        menu: &Menu,
        position: (f64, f64),
    ) -> Result<bool> {
        let window = self
            .windows
            .get_mut(id)
            .ok_or_else(|| anyhow!("window {:?} is not open", id))?;
        menu.show_context_menu(&window.window, position)
            .context("failed to show context menu")
    }

    /// Sends a typed event to a window's JavaScript bridge.
    pub fn emit(
        &mut self,
        id: WindowId,
        channel: impl Into<String>,
        payload: impl Serialize,
    ) -> Result<()> {
        self.windows.emit(id, channel.into(), payload)
    }

    /// Reloads a window's WebView.
    pub fn reload(&mut self, id: WindowId) -> Result<()> {
        self.windows.webview(id)?.reload()
    }

    /// Navigates a window's WebView to a URL.
    pub fn navigate(&mut self, id: WindowId, url: &str) -> Result<()> {
        self.windows.webview(id)?.navigate(url)
    }

    /// Evaluates JavaScript in a window's WebView.
    pub fn evaluate_script(&mut self, id: WindowId, script: &str) -> Result<()> {
        self.windows.webview(id)?.evaluate_script(script)
    }

    /// Returns a page-level handle for one window's WebView.
    pub fn page(&mut self, id: WindowId) -> Result<PageHandle<'_>> {
        Ok(self.windows.webview(id)?.page())
    }

    /// Asynchronously resizes a window to the current document's content size.
    ///
    /// JavaScript reports the content dimensions through an internal IPC
    /// channel; the native resize occurs on a later event-loop turn.
    pub fn fit_content(&mut self, id: WindowId) -> Result<()> {
        self.windows.request_fit_content(id)
    }

    /// Restricts pointer input to the supplied logical rectangles.
    ///
    /// Implemented for GTK on Linux when the display advertises input-shape
    /// support. Unsupported platforms return an explicit error.
    pub fn set_input_region(&mut self, id: WindowId, rects: &[Rect]) -> Result<()> {
        self.windows.set_input_region(id, rects)
    }

    /// Delivers a deep-link URL to registered open-URL callbacks.
    ///
    /// This manual entry point is useful on Linux without a desktop scheme
    /// registration, and for tests. The URL scheme must have been registered
    /// with [`App::register_deep_link`].
    pub fn deliver_open_url(&mut self, url: impl Into<String>) -> Result<()> {
        let url = url.into();
        self.windows
            .deep_links
            .validate(&url)
            .context("invalid deep link")?;
        self.proxy
            .send_event(UserEvent::OpenUrl(url))
            .map_err(|_| anyhow!("native event loop is no longer available"))
    }

    /// Requests application exit.
    pub fn quit(&mut self) {
        let _ = self.proxy.send_event(UserEvent::Quit);
    }
}

struct ManagedWindow {
    id: WindowId,
    title: String,
    window_key: Option<String>,
    webview: Option<WebViewHandle>,
    menu: Option<Menu>,
    window: Window,
    fullscreen: bool,
    decorations: bool,
}

struct WindowManager {
    windows: HashMap<WindowId, ManagedWindow>,
    tao_ids: HashMap<TaoWindowId, WindowId>,
    next_id: u64,
    ipc_router: Arc<IpcRouter>,
    proxy: EventLoopProxy<UserEvent>,
    app_menu: Option<Menu>,
    file_drop_enabled: bool,
    app_id: String,
    window_state_store: Option<Arc<dyn WindowStateStore>>,
    deep_links: DeepLinkRegistry,
}

impl WindowManager {
    fn new(
        ipc_router: Arc<IpcRouter>,
        proxy: EventLoopProxy<UserEvent>,
        app_menu: Option<Menu>,
        file_drop_enabled: bool,
        app_id: String,
        window_state_store: Option<Arc<dyn WindowStateStore>>,
        deep_links: DeepLinkRegistry,
    ) -> Self {
        Self {
            windows: HashMap::new(),
            tao_ids: HashMap::new(),
            next_id: 1,
            ipc_router,
            proxy,
            app_menu,
            file_drop_enabled,
            app_id,
            window_state_store,
            deep_links,
        }
    }

    fn create_window(
        &mut self,
        target: &EventLoopWindowTarget<UserEvent>,
        config: WindowConfig,
    ) -> Result<WindowId> {
        let id = WindowId(self.next_id);
        self.next_id += 1;
        let window_key = config.window_key.clone();
        let restored_state = self.load_window_state(window_key.as_deref());
        let width = restored_state
            .as_ref()
            .map(|state| state.width)
            .unwrap_or(config.width);
        let height = restored_state
            .as_ref()
            .map(|state| state.height)
            .unwrap_or(config.height);
        let position = restored_state
            .as_ref()
            .map(|state| (state.x, state.y))
            .or(config.position);
        let fullscreen = restored_state
            .as_ref()
            .map(|state| state.fullscreen)
            .unwrap_or(config.fullscreen);
        let transparent = config.transparent;

        let mut builder = WindowBuilder::new()
            .with_title(&config.title)
            .with_inner_size(LogicalSize::new(width, height))
            .with_decorations(config.decorations)
            .with_resizable(config.resizable)
            .with_visible(config.visible)
            .with_transparent(config.transparent)
            .with_always_on_top(config.always_on_top);

        if let Some((width, height)) = config.min_size {
            builder = builder.with_min_inner_size(LogicalSize::new(width, height));
        }
        if let Some((x, y)) = position {
            builder = builder.with_position(LogicalPosition::new(x, y));
        }
        if fullscreen {
            builder = builder.with_fullscreen(Some(Fullscreen::Borderless(None)));
        } else if restored_state
            .as_ref()
            .map(|state| state.maximized)
            .unwrap_or(false)
        {
            builder = builder.with_maximized(true);
        }

        #[cfg(target_os = "macos")]
        if let Some(buttons) = config.title_buttons {
            if !buttons.close && !buttons.minimize && !buttons.maximize {
                use tao::platform::macos::WindowBuilderExtMacOS;
                builder = builder.with_titlebar_buttons_hidden(true);
            }
        }

        let window = builder
            .build(target)
            .with_context(|| format!("failed to create window `{}`", config.title))?;
        apply_platform_window_options(&window, &config)?;
        let tao_id = window.id();
        let menu = self.app_menu.clone();
        if let Some(menu) = &menu {
            menu.attach_to_window(&window)
                .context("failed to attach application menu")?;
        }

        let webview = if let Some(mut webview_config) = config.webview {
            let direct_file_drop = webview_config.on_file_drop.clone();
            if self.file_drop_enabled || direct_file_drop.is_some() {
                let proxy = self.proxy.clone();
                webview_config.on_file_drop = Some(Arc::new(move |event| {
                    if let Some(callback) = &direct_file_drop {
                        callback(event.clone());
                    }
                    if proxy
                        .send_event(UserEvent::FileDrop {
                            window_id: id,
                            event,
                        })
                        .is_err()
                    {
                        tracing::warn!(?id, "failed to deliver file drop event");
                    }
                }));
            }

            let router = self.ipc_router.clone();
            let proxy = self.proxy.clone();
            let ipc_handler: IpcMessageHandler = Arc::new(move |body| {
                if let Some((width, height)) = parse_fit_content_request(&body) {
                    if proxy
                        .send_event(UserEvent::FitContent {
                            window_id: id,
                            width,
                            height,
                        })
                        .is_err()
                    {
                        tracing::warn!(?id, "failed to deliver fit-content size");
                    }
                    return;
                }

                let reply = router.dispatch_json(&body);
                if proxy
                    .send_event(UserEvent::IpcReply {
                        window_id: id,
                        reply,
                    })
                    .is_err()
                {
                    tracing::warn!(?id, "failed to deliver IPC reply");
                }
            });

            Some(
                WebViewHandle::new(&window, webview_config, ipc_handler, transparent)
                    .context("failed to create WebView")?,
            )
        } else {
            None
        };

        self.tao_ids.insert(tao_id, id);
        self.windows.insert(
            id,
            ManagedWindow {
                id,
                title: config.title,
                window_key,
                webview,
                menu,
                window,
                fullscreen,
                decorations: config.decorations,
            },
        );

        Ok(id)
    }

    fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    fn contains(&self, id: WindowId) -> bool {
        self.windows.contains_key(&id)
    }

    fn get_mut(&mut self, id: WindowId) -> Option<&mut ManagedWindow> {
        self.windows.get_mut(&id)
    }

    fn list(&self) -> Vec<WindowInfo> {
        let mut windows = self
            .windows
            .values()
            .map(|window| {
                let size = window
                    .window
                    .inner_size()
                    .to_logical(window.window.scale_factor());
                WindowInfo {
                    id: window.id,
                    title: window.title.clone(),
                    width: size.width,
                    height: size.height,
                    fullscreen: window.fullscreen,
                    decorations: window.decorations,
                    has_webview: window.webview.is_some(),
                }
            })
            .collect::<Vec<_>>();
        windows.sort_by_key(|window| window.id.0);
        windows
    }

    fn close(&mut self, id: WindowId) -> bool {
        let Some(window) = self.windows.remove(&id) else {
            return false;
        };
        self.save_window_state(&window);
        self.tao_ids.remove(&window.window.id());
        true
    }

    fn close_by_tao_id(&mut self, tao_id: TaoWindowId) -> bool {
        let Some(id) = self.tao_ids.get(&tao_id).copied() else {
            return false;
        };
        self.close(id)
    }

    fn webview(&mut self, id: WindowId) -> Result<&WebViewHandle> {
        self.windows
            .get(&id)
            .ok_or_else(|| anyhow!("window {:?} is not open", id))?
            .webview
            .as_ref()
            .ok_or_else(|| anyhow!("window {:?} has no WebView", id))
    }

    fn send_reply(&mut self, id: WindowId, reply: &IpcReply) {
        let script = match ipc::reply_script(reply) {
            Ok(script) => script,
            Err(error) => {
                tracing::error!(%error, "failed to serialize IPC reply");
                return;
            }
        };

        if let Ok(webview) = self.webview(id) {
            if let Err(error) = webview.evaluate_script(&script) {
                tracing::error!(%error, ?id, "failed to deliver IPC reply");
            }
        }
    }

    fn emit(&mut self, id: WindowId, channel: String, payload: impl Serialize) -> Result<()> {
        let payload =
            serde_json::to_value(payload).context("failed to serialize IPC event payload")?;
        let script =
            ipc::event_script(&channel, &payload).context("failed to build IPC event script")?;
        self.webview(id)?.evaluate_script(&script)
    }

    fn reload_all(&mut self) {
        for window in self.windows.values() {
            if let Some(webview) = &window.webview {
                if let Err(error) = webview.reload() {
                    tracing::error!(%error, id = ?window.id, "failed to reload WebView");
                }
            }
        }
    }

    fn request_fit_content(&mut self, id: WindowId) -> Result<()> {
        self.webview(id)?
            .evaluate_script(FIT_CONTENT_SCRIPT)
            .context("failed to request WebView content size")
    }

    fn set_input_region(&mut self, id: WindowId, rects: &[Rect]) -> Result<()> {
        let window = self
            .windows
            .get(&id)
            .ok_or_else(|| anyhow!("window {:?} is not open", id))?;
        apply_input_region(&window.window, rects)
    }

    fn load_window_state(&self, window_key: Option<&str>) -> Option<WindowState> {
        let store = self.window_state_store.as_ref()?;
        let window_key = window_key?;
        match store.load(&self.app_id, window_key) {
            Ok(state) => state,
            Err(error) => {
                tracing::warn!(
                    %error,
                    app_id = %self.app_id,
                    window_key,
                    "failed to load window state"
                );
                None
            }
        }
    }

    fn save_window_state(&self, window: &ManagedWindow) {
        let (Some(store), Some(window_key)) = (&self.window_state_store, &window.window_key) else {
            return;
        };

        let scale = window.window.scale_factor();
        let position = window
            .window
            .outer_position()
            .ok()
            .map(|position| position.to_logical::<f64>(scale))
            .unwrap_or_else(|| LogicalPosition::new(0.0, 0.0));
        let size = window.window.inner_size().to_logical::<f64>(scale);
        let state = WindowState {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
            maximized: window.window.is_maximized(),
            fullscreen: window.window.fullscreen().is_some(),
            display_scale: scale,
        };

        if let Err(error) = store.save(&self.app_id, window_key, &state) {
            tracing::warn!(
                %error,
                app_id = %self.app_id,
                window_key,
                "failed to save window state"
            );
        }
    }
}

fn apply_platform_window_options(window: &Window, config: &WindowConfig) -> Result<()> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    if config.skip_taskbar {
        #[cfg(target_os = "linux")]
        {
            use tao::platform::unix::WindowExtUnix;
            window
                .set_skip_taskbar(true)
                .context("failed to hide window from the Linux taskbar")?;
        }
        #[cfg(target_os = "windows")]
        {
            use tao::platform::windows::WindowExtWindows;
            window
                .set_skip_taskbar(true)
                .context("failed to hide window from the Windows taskbar")?;
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (window, config);
    }

    Ok(())
}

fn apply_input_region(window: &Window, rects: &[Rect]) -> Result<()> {
    if rects.iter().any(|rect| rect.width <= 0 || rect.height <= 0) {
        bail!("input-region rectangles must have positive width and height");
    }

    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;
        use tao::platform::unix::WindowExtUnix;

        let gtk_window = window.gtk_window();
        let display = gtk_window.display();
        if !display.supports_input_shapes() {
            bail!("the active display does not support input shapes");
        }

        let rectangles = rects
            .iter()
            .map(|rect| gtk::cairo::RectangleInt::new(rect.x, rect.y, rect.width, rect.height))
            .collect::<Vec<_>>();
        let region = gtk::cairo::Region::create_rectangles(&rectangles);
        gtk_window.input_shape_combine_region(Some(&region));
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, rects);
        bail!("input regions are not supported on this platform")
    }
}

fn parse_fit_content_request(body: &str) -> Option<(f64, f64)> {
    let request = serde_json::from_str::<ipc::IpcRequest>(body).ok()?;
    if request.channel != FIT_CONTENT_CHANNEL {
        return None;
    }

    let width = request.payload.get("width")?.as_f64()?;
    let height = request.payload.get("height")?.as_f64()?;
    (width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0)
        .then_some((width, height))
}

fn handle_last_window_closed(
    target: &EventLoopWindowTarget<UserEvent>,
    windows: &mut WindowManager,
    proxy: &EventLoopProxy<UserEvent>,
    callbacks: &mut [LifecycleCallback],
    exit_when_last_window_closes: bool,
    control_flow: &mut ControlFlow,
) {
    if !windows.is_empty() {
        return;
    }

    let mut context = AppContext::new(target, windows, proxy);
    for callback in callbacks {
        if let Err(error) = callback(&mut context) {
            tracing::error!(%error, "on_all_closed callback failed");
        }
    }

    if exit_when_last_window_closes {
        *control_flow = ControlFlow::Exit;
    }
}

#[derive(Debug)]
enum UserEvent {
    CloseWindow(WindowId),
    IpcReply {
        window_id: WindowId,
        reply: IpcReply,
    },
    Menu(MenuEvent),
    FileDrop {
        window_id: WindowId,
        event: FileDropEvent,
    },
    OpenUrl(String),
    FitContent {
        window_id: WindowId,
        width: f64,
        height: f64,
    },
    ReloadAll,
    Quit,
}

struct ReloadWatcher {
    _watcher: RecommendedWatcher,
}

impl ReloadWatcher {
    fn spawn(path: PathBuf, proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        if !path.exists() {
            bail!("watch path does not exist: {}", path.display());
        }

        let mut watcher = notify::recommended_watcher(
            move |result: notify::Result<notify::Event>| match result {
                Ok(event) => {
                    if matches!(event.kind, EventKind::Access(_)) {
                        return;
                    }
                    if proxy.send_event(UserEvent::ReloadAll).is_err() {
                        tracing::debug!("hot-reload watcher stopped");
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "hot-reload watcher reported an error");
                }
            },
        )
        .context("failed to create native filesystem watcher")?;
        watcher
            .watch(&path, RecursiveMode::Recursive)
            .with_context(|| format!("failed to watch {}", path.display()))?;

        Ok(Self { _watcher: watcher })
    }
}

struct MenuEventForwarder {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MenuEventForwarder {
    fn spawn(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let receiver = menu::subscribe();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let handle = thread::Builder::new()
            .name("myrust-menu-events".to_string())
            .spawn(move || {
                while !thread_stop.load(Ordering::Relaxed) {
                    match receiver.recv_timeout(Duration::from_millis(250)) {
                        Ok(event) => {
                            if proxy.send_event(UserEvent::Menu(event)).is_err() {
                                break;
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            break;
                        }
                    }
                }
            })
            .context("failed to spawn menu event forwarding thread")?;

        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for MenuEventForwarder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
