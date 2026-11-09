//! Application lifecycle and multi-window management.
//!
//! One [`App`] instance owns the native event loop, all windows, the typed IPC
//! router, and optional development reload watchers.

use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    window::{Fullscreen, Window, WindowBuilder, WindowId as TaoWindowId},
};

use crate::{
    ipc::{self, IpcReply, IpcRouter},
    webview::{IpcMessageHandler, WebViewConfig, WebViewHandle},
};

static APP_CREATED: AtomicBool = AtomicBool::new(false);

/// Stable application-managed window identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowId(
    /// Raw application window value.
    pub u64,
);

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
    exit_when_last_window_closes: bool,
    watch_paths: Vec<PathBuf>,
}

type LifecycleCallback = Box<dyn for<'a> FnMut(&mut AppContext<'a>) -> Result<()> + 'static>;
type ActivateCallback = Box<dyn for<'a> FnMut(&mut AppContext<'a>, bool) -> Result<()> + 'static>;

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

    /// Controls whether the app exits after its final window closes.
    pub fn exit_when_last_window_closes(&mut self, enabled: bool) -> &mut Self {
        self.exit_when_last_window_closes = enabled;
        self
    }

    /// Watches a file or directory and reloads all WebViews after changes.
    ///
    /// The watcher polls metadata every 500ms and is intended for local
    /// development rather than hot paths.
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
        let mut windows = WindowManager::new(ipc_router, proxy.clone());

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

        tracing::info!(app = %app_name, "starting native event loop");
        event_loop.run(move |event, target, control_flow| {
            let _keep_watchers = &watchers;
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

    /// Requests application exit.
    pub fn quit(&mut self) {
        let _ = self.proxy.send_event(UserEvent::Quit);
    }
}

struct ManagedWindow {
    id: WindowId,
    title: String,
    webview: Option<WebViewHandle>,
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
}

impl WindowManager {
    fn new(ipc_router: Arc<IpcRouter>, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            windows: HashMap::new(),
            tao_ids: HashMap::new(),
            next_id: 1,
            ipc_router,
            proxy,
        }
    }

    fn create_window(
        &mut self,
        target: &EventLoopWindowTarget<UserEvent>,
        config: WindowConfig,
    ) -> Result<WindowId> {
        let id = WindowId(self.next_id);
        self.next_id += 1;

        let mut builder = WindowBuilder::new()
            .with_title(&config.title)
            .with_inner_size(LogicalSize::new(config.width, config.height))
            .with_decorations(config.decorations)
            .with_resizable(config.resizable)
            .with_visible(config.visible);

        if let Some((width, height)) = config.min_size {
            builder = builder.with_min_inner_size(LogicalSize::new(width, height));
        }
        if let Some((x, y)) = config.position {
            builder = builder.with_position(LogicalPosition::new(x, y));
        }
        if config.fullscreen {
            builder = builder.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }

        let window = builder
            .build(target)
            .with_context(|| format!("failed to create window `{}`", config.title))?;
        let tao_id = window.id();

        let webview = if let Some(webview_config) = config.webview {
            let router = self.ipc_router.clone();
            let proxy = self.proxy.clone();
            let ipc_handler: IpcMessageHandler = Arc::new(move |body| {
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
                WebViewHandle::new(&window, webview_config, ipc_handler)
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
                webview,
                window,
                fullscreen: config.fullscreen,
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
    ReloadAll,
    Quit,
}

struct ReloadWatcher {
    stop: Arc<AtomicBool>,
}

impl ReloadWatcher {
    fn spawn(path: PathBuf, proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        if !path.exists() {
            bail!("watch path does not exist: {}", path.display());
        }

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        thread::Builder::new()
            .name("myrust-hot-reload".to_string())
            .spawn(move || {
                let mut snapshot = snapshot_path(&path).unwrap_or_default();
                while !thread_stop.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(500));
                    let Ok(next) = snapshot_path(&path) else {
                        continue;
                    };
                    if next != snapshot {
                        snapshot = next;
                        if proxy.send_event(UserEvent::ReloadAll).is_err() {
                            break;
                        }
                    }
                }
            })
            .context("failed to spawn hot-reload watcher thread")?;

        Ok(Self { stop })
    }
}

impl Drop for ReloadWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn snapshot_path(path: &Path) -> Result<HashMap<PathBuf, (u64, u128)>> {
    let mut snapshot = HashMap::new();
    collect_snapshot(path, &mut snapshot)?;
    Ok(snapshot)
}

fn collect_snapshot(path: &Path, snapshot: &mut HashMap<PathBuf, (u64, u128)>) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| {
            time.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_nanos())
        })
        .unwrap_or_default();
    snapshot.insert(path.to_path_buf(), (metadata.len(), modified));

    if metadata.is_dir() {
        for entry in fs::read_dir(path)
            .with_context(|| format!("failed to read directory {}", path.display()))?
        {
            let entry = entry?;
            collect_snapshot(&entry.path(), snapshot)?;
        }
    }

    Ok(())
}
