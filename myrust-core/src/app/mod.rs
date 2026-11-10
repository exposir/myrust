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
    num::NonZeroU32,
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
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    keyboard::{Key as TaoKey, ModifiersState},
    window::{Fullscreen, Window, WindowBuilder, WindowId as TaoWindowId},
};

use crate::{
    desktop::menu::{self, Menu, MenuEvent},
    ipc::{self, IpcReply, IpcRouter},
    ui::{
        create_renderer, CpuFrame, CpuRasterizer, DisplayList, DrawCommand, InputState,
        InputTranslator, Key, NativeInput, NativeUi, PointerButton, RenderSize, RenderStats,
        Renderer, RendererBackend, RendererKind, Size, Transform,
    },
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

/// Content hosted by a native application window.
#[derive(Debug, Clone)]
pub enum WindowContent {
    /// Existing system WebView content.
    WebView(Box<WebViewConfig>),
    /// Event-driven native UI drawn by the application runtime.
    NativeUi(NativeUi),
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
    ///
    /// This legacy field remains supported. New code may select either
    /// [`WindowContent::WebView`] or [`WindowContent::NativeUi`] through
    /// [`WindowConfig::content`]. Setting both fields is an error.
    pub webview: Option<WebViewConfig>,
    /// Optional unified window content selection.
    pub content: Option<WindowContent>,
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
            content: None,
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
    /// Whether the window hosts native self-drawn UI.
    pub has_native_ui: bool,
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

/// Disables WebKitGTK's DMA-BUF compositing renderer on Linux systems without a
/// DRM device. There is no GPU to share those buffers with, and the renderer
/// otherwise costs tens of megabytes of anonymous memory. An explicit
/// `WEBKIT_DISABLE_DMABUF_RENDERER` setting is left untouched.
#[cfg(target_os = "linux")]
fn configure_linux_webview_memory() {
    if env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some() || Path::new("/dev/dri").exists() {
        return;
    }
    env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
}

impl App {
    /// Creates the process singleton application.
    ///
    /// A second call in the same process returns an error.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        if APP_CREATED.swap(true, Ordering::SeqCst) {
            bail!("myrust App is already created in this process");
        }

        #[cfg(target_os = "linux")]
        configure_linux_webview_memory();

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
                Event::WindowEvent {
                    window_id, event, ..
                } => {
                    windows.handle_native_ui_window_event(window_id, &event);
                }
                Event::RedrawRequested(window_id) => {
                    windows.redraw_native_ui_by_tao_id(window_id);
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
                Event::UserEvent(UserEvent::NativeUiRepaint(window_id)) => {
                    if let Some(window) = windows.get_mut(window_id) {
                        if window.native_ui.is_some() {
                            window.window.request_redraw();
                        }
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

    /// Returns renderer statistics for a native UI window.
    pub fn native_ui_stats(&self, id: WindowId) -> Result<RenderStats> {
        self.windows.native_ui_stats(id)
    }

    /// Returns the selected renderer backend for a native UI window.
    pub fn native_ui_backend(&self, id: WindowId) -> Result<RendererBackend> {
        self.windows.native_ui_backend(id)
    }

    /// Returns the most recent native UI renderer or presentation error.
    pub fn native_ui_last_error(&self, id: WindowId) -> Result<Option<String>> {
        self.windows.native_ui_last_error(id)
    }

    /// Renders and presents one native UI window immediately.
    pub fn redraw_native_ui(&mut self, id: WindowId) -> Result<()> {
        self.windows.render_native_ui(id)
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
    native_ui: Option<NativeUiWindow>,
    menu: Option<Menu>,
    window: Window,
    fullscreen: bool,
    decorations: bool,
}

struct NativeUiWindow {
    ui: NativeUi,
    renderer: Box<dyn Renderer>,
    translator: InputTranslator,
    modifiers: ModifiersState,
    physical_size: RenderSize,
    logical_size: Size,
    scale_factor: f64,
    cpu_frame: Option<CpuFrame>,
    last_stats: RenderStats,
    last_input_consumed: bool,
    needs_animation_frame: bool,
    access_nodes_logged: bool,
    fallback_reason: Option<String>,
    last_error: Option<String>,
}

impl NativeUiWindow {
    fn new(ui: NativeUi, physical_size: RenderSize, scale_factor: f64) -> Result<Self> {
        let physical_size = normalize_render_size(physical_size);
        let scale_factor = normalize_scale_factor(scale_factor);
        let requested_renderer = ui.renderer_kind();
        let (renderer, fallback_reason) = match create_renderer(requested_renderer, physical_size) {
            Ok(renderer) => (renderer, None),
            Err(error) => {
                tracing::warn!(
                    requested = %requested_renderer,
                    %error,
                    "native UI renderer unavailable; falling back to software"
                );
                let renderer = create_renderer(RendererKind::Software, physical_size)
                    .context("failed to create native UI software fallback")?;
                (
                    renderer,
                    Some(format!(
                        "{requested_renderer} renderer unavailable: {error}"
                    )),
                )
            }
        };
        let logical_size = physical_to_logical(physical_size, scale_factor);
        let mut native = Self {
            ui,
            renderer,
            translator: InputTranslator::new(scale_factor),
            modifiers: ModifiersState::empty(),
            physical_size,
            logical_size,
            scale_factor,
            cpu_frame: None,
            last_stats: RenderStats::default(),
            last_input_consumed: false,
            needs_animation_frame: false,
            access_nodes_logged: false,
            fallback_reason,
            last_error: None,
        };
        if let Err(error) = native.render() {
            tracing::warn!(%error, "initial native UI render failed");
            native.last_error = Some(error);
        }
        Ok(native)
    }

    fn render(&mut self) -> std::result::Result<(), String> {
        let frame = self
            .ui
            .render(self.logical_size)
            .map_err(|error| error.to_string())?;
        self.last_input_consumed = frame.input_consumed;
        self.needs_animation_frame = frame.needs_animation_frame;
        if !self.access_nodes_logged && !frame.access_nodes.is_empty() {
            tracing::info!(
                count = frame.access_nodes.len(),
                "native UI accessibility metadata produced (platform AT bridge not implemented)"
            );
            self.access_nodes_logged = true;
        }
        let display = scaled_display(&frame.display, self.scale_factor as f32);
        let background = self.ui.theme().background;
        let cpu_frame = CpuRasterizer::rasterize(&display, self.physical_size, background)
            .map_err(|error| error.to_string())?;
        self.cpu_frame = Some(cpu_frame);
        match self.renderer.render(&display) {
            Ok(stats) => {
                self.last_stats = stats;
                self.last_error = None;
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                self.last_error = Some(message.clone());
                Err(message)
            }
        }
    }

    fn resize(&mut self, physical_size: RenderSize, scale_factor: f64) -> bool {
        let physical_size = normalize_render_size(physical_size);
        let scale_factor = normalize_scale_factor(scale_factor);
        if physical_size == self.physical_size && scale_factor == self.scale_factor {
            return false;
        }
        self.physical_size = physical_size;
        self.logical_size = physical_to_logical(physical_size, scale_factor);
        self.scale_factor = scale_factor;
        self.translator.set_scale_factor(scale_factor);
        if let Err(error) = self.renderer.resize(physical_size) {
            tracing::warn!(%error, "native UI renderer resize failed");
            self.last_error = Some(error.to_string());
            return true;
        }
        true
    }

    fn effective_error(&self) -> Option<String> {
        self.last_error
            .clone()
            .or_else(|| self.fallback_reason.clone())
    }

    fn handle_window_event(&mut self, event: &WindowEvent<'_>) -> bool {
        match event {
            WindowEvent::Resized(size) => {
                return self.resize(RenderSize::new(size.width, size.height), self.scale_factor);
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                new_inner_size,
            } => {
                return self.resize(
                    RenderSize::new(new_inner_size.width, new_inner_size.height),
                    *scale_factor,
                );
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = *modifiers;
                return false;
            }
            WindowEvent::CursorMoved { position, .. } => {
                return self.dispatch_input(NativeInput::CursorMoved {
                    x: position.x,
                    y: position.y,
                });
            }
            WindowEvent::MouseInput { state, button, .. } => {
                return self.dispatch_input(NativeInput::MouseInput {
                    state: if matches!(state, ElementState::Pressed) {
                        InputState::Pressed
                    } else {
                        InputState::Released
                    },
                    button: pointer_button(*button),
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (delta_x, delta_y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 20.0, y * 20.0),
                    MouseScrollDelta::PixelDelta(position) => (
                        (position.x / self.scale_factor) as f32,
                        (position.y / self.scale_factor) as f32,
                    ),
                    _ => (0.0, 0.0),
                };
                return self.dispatch_input(NativeInput::Wheel { delta_x, delta_y });
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let mut dispatched = false;
                let key = map_tao_key(&event.logical_key, self.modifiers.shift_key());
                if let Some(key) = key {
                    dispatched |= self.dispatch_input(NativeInput::Key { key });
                }
                if let Some(text) = event.text {
                    if key.map(key_accepts_text).unwrap_or(true) {
                        dispatched |= self.dispatch_input(NativeInput::Text(text.to_string()));
                    }
                }
                return dispatched;
            }
            _ => {}
        }
        false
    }

    fn dispatch_input(&mut self, input: NativeInput) -> bool {
        let Some(event) = self.translator.translate(&input) else {
            return false;
        };
        match self.ui.input(event) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "native UI input queueing failed");
                self.last_error = Some(error.to_string());
                false
            }
        }
    }
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
        let mut config = config;
        let content = resolve_window_content(config.content.take(), config.webview.take())?;
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

        let mut webview = None;
        let mut native_ui = None;
        match content {
            Some(WindowContent::WebView(webview_config)) => {
                let mut webview_config = *webview_config;
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

                webview = Some(
                    WebViewHandle::new(&window, webview_config, ipc_handler, transparent)
                        .context("failed to create WebView")?,
                );
            }
            Some(WindowContent::NativeUi(ui)) => {
                let scale_factor = window.scale_factor();
                let physical_size = window.inner_size();
                native_ui = Some(
                    NativeUiWindow::new(
                        ui,
                        RenderSize::new(physical_size.width, physical_size.height),
                        scale_factor,
                    )
                    .context("failed to initialize native UI window")?,
                );
            }
            None => {}
        }

        #[cfg(target_os = "linux")]
        if native_ui.is_some() {
            use gtk::prelude::WidgetExt;
            use tao::platform::unix::WindowExtUnix;

            // GTK paints its window background over the frame presented right after
            // creation, so every GTK draw asks for a fresh native frame.
            let proxy = self.proxy.clone();
            window.gtk_window().connect_draw(move |_, _| {
                if proxy.send_event(UserEvent::NativeUiRepaint(id)).is_err() {
                    tracing::warn!(?id, "failed to schedule native UI repaint after draw");
                }
                gtk::glib::Propagation::Proceed
            });
        }

        self.tao_ids.insert(tao_id, id);
        self.windows.insert(
            id,
            ManagedWindow {
                id,
                title: config.title,
                window_key,
                webview,
                native_ui,
                menu,
                window,
                fullscreen,
                decorations: config.decorations,
            },
        );
        if let Some(window) = self.windows.get(&id) {
            if window.native_ui.is_some() {
                window.window.request_redraw();
            }
        }

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
                    has_native_ui: window.native_ui.is_some(),
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

    fn native_ui(&self, id: WindowId) -> Result<&NativeUiWindow> {
        self.windows
            .get(&id)
            .ok_or_else(|| anyhow!("window {:?} is not open", id))?
            .native_ui
            .as_ref()
            .ok_or_else(|| anyhow!("window {:?} has no native UI", id))
    }

    fn native_ui_stats(&self, id: WindowId) -> Result<RenderStats> {
        Ok(self.native_ui(id)?.last_stats.clone())
    }

    fn native_ui_backend(&self, id: WindowId) -> Result<RendererBackend> {
        Ok(self.native_ui(id)?.renderer.backend())
    }

    fn native_ui_last_error(&self, id: WindowId) -> Result<Option<String>> {
        Ok(self.native_ui(id)?.effective_error())
    }

    fn render_native_ui(&mut self, id: WindowId) -> Result<()> {
        let window = self
            .windows
            .get_mut(&id)
            .ok_or_else(|| anyhow!("window {:?} is not open", id))?;
        let native = window
            .native_ui
            .as_mut()
            .ok_or_else(|| anyhow!("window {:?} has no native UI", id))?;
        let render_result = native.render();
        let present_result = native
            .cpu_frame
            .as_ref()
            .ok_or_else(|| anyhow!("native UI produced no CPU frame"))
            .and_then(|frame| present_cpu_frame(&window.window, frame));
        if let Err(error) = &present_result {
            native.last_error = Some(error.to_string());
        }
        render_result.map_err(anyhow::Error::msg)?;
        present_result?;
        if native.last_input_consumed || native.needs_animation_frame {
            window.window.request_redraw();
        }
        Ok(())
    }

    fn handle_native_ui_window_event(&mut self, tao_id: TaoWindowId, event: &WindowEvent<'_>) {
        let Some(id) = self.tao_ids.get(&tao_id).copied() else {
            return;
        };
        if let Some(window) = self.windows.get_mut(&id) {
            let Some(native) = window.native_ui.as_mut() else {
                return;
            };
            if native.handle_window_event(event) {
                window.window.request_redraw();
            }
        }
    }

    fn redraw_native_ui_by_tao_id(&mut self, tao_id: TaoWindowId) {
        let Some(id) = self.tao_ids.get(&tao_id).copied() else {
            return;
        };
        if let Err(error) = self.render_native_ui(id) {
            tracing::error!(%error, ?id, "native UI redraw failed");
        }
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

fn resolve_window_content(
    content: Option<WindowContent>,
    legacy_webview: Option<WebViewConfig>,
) -> Result<Option<WindowContent>> {
    match (content, legacy_webview) {
        (Some(content), None) => Ok(Some(content)),
        (None, Some(webview)) => Ok(Some(WindowContent::WebView(Box::new(webview)))),
        (Some(_), Some(_)) => {
            bail!("WindowConfig cannot set both `content` and legacy `webview`")
        }
        (None, None) => Ok(None),
    }
}

fn scaled_display(display: &DisplayList, scale_factor: f32) -> DisplayList {
    let mut commands = display.commands.clone();
    if scale_factor.is_finite() && (scale_factor - 1.0).abs() > f32::EPSILON {
        commands.insert(
            0,
            DrawCommand::TransformPush {
                transform: Transform::scale(crate::ui::Point::new(scale_factor, scale_factor)),
            },
        );
        commands.push(DrawCommand::TransformPop);
    }
    DisplayList::from_commands(commands)
}

fn present_cpu_frame(window: &Window, frame: &CpuFrame) -> Result<()> {
    let width =
        NonZeroU32::new(frame.width).ok_or_else(|| anyhow!("native UI frame has zero width"))?;
    let height =
        NonZeroU32::new(frame.height).ok_or_else(|| anyhow!("native UI frame has zero height"))?;
    let context = softbuffer::Context::new(window)
        .map_err(|error| anyhow!("failed to create native software display context: {error:?}"))?;
    let mut surface = softbuffer::Surface::new(&context, window)
        .map_err(|error| anyhow!("failed to create native software window surface: {error:?}"))?;
    surface
        .resize(width, height)
        .map_err(|error| anyhow!("failed to resize native software surface: {error:?}"))?;
    let mut buffer = surface
        .buffer_mut()
        .map_err(|error| anyhow!("failed to acquire native software buffer: {error:?}"))?;
    buffer.copy_from_slice(&frame.pixels);
    buffer
        .present()
        .map_err(|error| anyhow!("failed to present native software frame: {error:?}"))
}

fn physical_to_logical(size: RenderSize, scale_factor: f64) -> Size {
    Size::new(
        size.width as f32 / scale_factor as f32,
        size.height as f32 / scale_factor as f32,
    )
}

fn normalize_render_size(size: RenderSize) -> RenderSize {
    RenderSize::new(size.width.max(1), size.height.max(1))
}

fn normalize_scale_factor(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Other(code) => PointerButton::Other(code),
        _ => PointerButton::Other(u16::MAX),
    }
}

fn map_tao_key(key: &TaoKey<'_>, shift: bool) -> Option<Key> {
    match key {
        TaoKey::Tab if shift => Some(Key::ShiftTab),
        TaoKey::Tab => Some(Key::Tab),
        TaoKey::Enter => Some(Key::Enter),
        TaoKey::Space => Some(Key::Space),
        TaoKey::Backspace => Some(Key::Backspace),
        TaoKey::Delete => Some(Key::Delete),
        TaoKey::ArrowLeft => Some(Key::ArrowLeft),
        TaoKey::ArrowRight => Some(Key::ArrowRight),
        TaoKey::Home => Some(Key::Home),
        TaoKey::End => Some(Key::End),
        _ => None,
    }
}

fn key_accepts_text(key: Key) -> bool {
    !matches!(
        key,
        Key::Tab
            | Key::ShiftTab
            | Key::Enter
            | Key::Backspace
            | Key::Delete
            | Key::ArrowLeft
            | Key::ArrowRight
            | Key::Home
            | Key::End
    )
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
    NativeUiRepaint(WindowId),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{button, column, text, NativeUi};

    fn test_native_ui() -> NativeUi {
        NativeUi::new(|context| {
            column(context).children(|context| {
                text(context, "native renderer");
                button(context, "action");
            });
        })
    }

    #[test]
    fn window_content_selects_native_ui_without_legacy_webview() {
        let content = resolve_window_content(Some(WindowContent::NativeUi(test_native_ui())), None)
            .unwrap()
            .unwrap();
        assert!(matches!(content, WindowContent::NativeUi(_)));
    }

    #[test]
    fn legacy_webview_field_still_resolves_to_webview_content() {
        let content = resolve_window_content(None, Some(WebViewConfig::default()))
            .unwrap()
            .unwrap();
        assert!(matches!(content, WindowContent::WebView(_)));
    }

    #[test]
    fn conflicting_content_and_webview_are_rejected() {
        let error = resolve_window_content(
            Some(WindowContent::NativeUi(test_native_ui())),
            Some(WebViewConfig::default()),
        )
        .unwrap_err();
        assert!(error.to_string().contains("cannot set both"));
    }

    #[test]
    fn native_ui_window_consumes_display_list_through_renderer() {
        let mut window =
            NativeUiWindow::new(test_native_ui(), RenderSize::new(320, 200), 1.0).unwrap();
        assert_eq!(window.renderer.backend(), RendererBackend::Software);
        assert_eq!(window.last_stats.total_commands, 3);
        assert!(window.cpu_frame.is_some());
        window.render().unwrap();
        assert_eq!(window.last_stats.command_counts.text, 2);
    }

    #[cfg(not(feature = "gpu"))]
    #[test]
    fn native_ui_auto_falls_back_without_gpu_feature() {
        let ui = test_native_ui().with_renderer_kind(crate::ui::RendererKind::Auto);
        let window = NativeUiWindow::new(ui, RenderSize::new(100, 80), 1.0).unwrap();
        assert_eq!(window.renderer.backend(), RendererBackend::Software);
    }

    #[cfg(not(feature = "gpu"))]
    #[test]
    fn explicit_wgpu_request_falls_back_and_reports_the_error() {
        let ui = test_native_ui().with_renderer_kind(crate::ui::RendererKind::Wgpu);
        let window = NativeUiWindow::new(ui, RenderSize::new(100, 80), 1.0).unwrap();
        assert_eq!(window.renderer.backend(), RendererBackend::Software);
        assert!(window
            .effective_error()
            .expect("fallback reason")
            .contains("wgpu renderer unavailable"));
    }

    #[test]
    fn tao_named_keys_map_to_navigation_keys() {
        assert_eq!(map_tao_key(&TaoKey::Tab, false), Some(Key::Tab));
        assert_eq!(map_tao_key(&TaoKey::Tab, true), Some(Key::ShiftTab));
        assert_eq!(map_tao_key(&TaoKey::Enter, false), Some(Key::Enter));
        assert_eq!(
            map_tao_key(&TaoKey::ArrowRight, false),
            Some(Key::ArrowRight)
        );
        assert_eq!(map_tao_key(&TaoKey::Character("x"), false), None);
        assert!(key_accepts_text(Key::Space));
        assert!(!key_accepts_text(Key::Enter));
    }
}
