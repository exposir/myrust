//! Wry-based system WebView wrapper.
//!
//! The wrapper uses WKWebView on macOS, WebView2 on Windows, and WebKitGTK on
//! Linux. A JavaScript IPC bridge is injected before application scripts run.

use std::{fmt, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tao::window::Window;
use wry::{DragDropEvent, PermissionKind, PermissionResponse, WebViewBuilder};

use crate::{
    desktop::{
        download::{DownloadEvent, DownloadHandler, DownloadPolicy, DownloadPolicyHandler},
        protocol::CustomProtocol,
    },
    ipc,
};

/// Page-level navigation, zoom, find, and rule APIs.
pub mod page;
/// Logical WebView sessions and state persistence hooks.
pub mod session;

pub use page::{
    ContentLoadType, ContentResourceType, ContentRule, ContentRuleAction, ContentRuleTrigger,
    FindOptions, PageHandle, PageOptions,
};
pub use session::{
    MemorySessionStateStore, ScrollPosition, Session, SessionHistoryEntry, SessionKind,
    SessionState, SessionStateStore,
};

/// Content loaded by a new WebView.
#[derive(Debug, Clone)]
pub enum PageSource {
    /// Navigate to a URL.
    Url(String),
    /// Render an HTML document directly as a string.
    Html(String),
}

impl Default for PageSource {
    fn default() -> Self {
        Self::Url("about:blank".to_string())
    }
}

/// Navigation policy for a WebView.
///
/// The policy is enforced through wry's navigation handler. Backends report
/// the initial document load there too, so `DenyAll` also blocks the starting
/// URL; choose a policy that admits the page you load.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum NavigationPolicy {
    /// Allow every navigation.
    #[default]
    AllowAll,
    /// Block every subsequent navigation.
    DenyAll,
    /// Allow `about:blank` and these hosts and their subdomains.
    AllowHosts(Vec<String>),
}

impl NavigationPolicy {
    /// Returns whether a navigation target is allowed.
    pub(crate) fn allows(&self, url: &str) -> bool {
        match self {
            Self::AllowAll => true,
            Self::DenyAll => false,
            Self::AllowHosts(hosts) => {
                if url.eq_ignore_ascii_case("about:blank") {
                    return true;
                }
                let Some(host) = host_of(url) else {
                    return false;
                };
                hosts.iter().any(|allowed| {
                    let allowed = allowed.trim().trim_end_matches('.').to_ascii_lowercase();
                    !allowed.is_empty()
                        && (host == allowed || host.ends_with(&format!(".{allowed}")))
                })
            }
        }
    }
}

/// Permission policy for a WebView.
///
/// The platform backend still decides which permission kinds it can ask
/// about; the policy only answers requests it receives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PermissionPolicy {
    /// Use the platform's prompt and persistence behavior.
    #[default]
    PlatformDefault,
    /// Deny every permission request.
    DenyAll,
    /// Allow every permission request.
    AllowAll,
}

/// Region-only WebView options layered over [`WebViewConfig`].
///
/// The public window WebView configuration stays unchanged; embedded regions
/// add initialization scripts, transparency and the navigation/permission
/// policies through this wrapper.
#[derive(Debug, Clone, Default)]
pub(crate) struct RegionPolicy {
    pub(crate) initialization_scripts: Vec<String>,
    pub(crate) transparent: bool,
    pub(crate) navigation: NavigationPolicy,
    pub(crate) permissions: PermissionPolicy,
}

/// WebView creation options.
#[derive(Clone, Default)]
pub struct WebViewConfig {
    /// Initial page URL or embedded HTML.
    pub source: PageSource,
    /// Enables platform developer tools where available.
    pub devtools: bool,
    /// Optional native file-drag callback for this WebView.
    pub on_file_drop: Option<FileDropHandler>,
    /// Page-level WebView options.
    pub page_options: PageOptions,
    /// Optional logical session bound to this page.
    pub session: Option<Session>,
    /// Optional download policy handler.
    pub download_policy: Option<DownloadPolicyHandler>,
    /// Optional download lifecycle callback.
    pub on_download: Option<DownloadHandler>,
    /// Custom scheme handlers for this WebView.
    pub protocols: Vec<CustomProtocol>,
    /// Marks the page as untrusted for future permission and navigation gates.
    ///
    /// This batch records the flag only. Wry 0.57 does not provide a single
    /// trusted/untrusted partition switch, so no permission gate is enforced
    /// yet.
    pub untrusted: bool,
}

impl fmt::Debug for WebViewConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebViewConfig")
            .field("source", &self.source)
            .field("devtools", &self.devtools)
            .field("on_file_drop", &self.on_file_drop.is_some())
            .field("page_options", &self.page_options)
            .field("session", &self.session)
            .field("download_policy", &self.download_policy.is_some())
            .field("on_download", &self.on_download.is_some())
            .field("protocols", &self.protocols)
            .field("untrusted", &self.untrusted)
            .finish()
    }
}

/// File drag phase reported by a WebView.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileDropKind {
    /// Files entered the WebView.
    Entered,
    /// Files are moving over the WebView.
    Over,
    /// Files were dropped onto the WebView.
    Dropped,
    /// The drag operation left the WebView or was cancelled.
    Cancelled,
}

/// File drag-and-drop event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDropEvent {
    /// Current drag phase.
    pub kind: FileDropKind,
    /// Absolute paths associated with enter and drop phases.
    pub paths: Vec<PathBuf>,
    /// Position relative to the WebView top-left corner in logical pixels.
    pub position: (f64, f64),
}

/// Callback invoked for native WebView file drag-and-drop events.
pub type FileDropHandler = Arc<dyn Fn(FileDropEvent) + Send + Sync + 'static>;

/// Callback invoked with the raw JSON body sent by `window.ipc.postMessage`.
pub(crate) type IpcMessageHandler = Arc<dyn Fn(String) + Send + Sync + 'static>;

/// Native WebView owned by a managed application window.
pub struct WebViewHandle {
    webview: wry::WebView,
    page_options: PageOptions,
    session: Option<Session>,
}

impl WebViewHandle {
    /// Creates a WebView inside a Tao window.
    pub(crate) fn new(
        parent: &Window,
        config: WebViewConfig,
        ipc_handler: IpcMessageHandler,
        transparent: bool,
    ) -> Result<Self> {
        Self::new_with_policy(
            parent,
            config,
            RegionPolicy::default(),
            ipc_handler,
            transparent,
        )
    }

    fn new_with_policy(
        parent: &Window,
        config: WebViewConfig,
        policy: RegionPolicy,
        ipc_handler: IpcMessageHandler,
        transparent: bool,
    ) -> Result<Self> {
        let (builder, page_options, session) =
            Self::prepare_builder(config, policy, ipc_handler, transparent);

        #[cfg(target_os = "linux")]
        let webview = {
            use tao::platform::unix::WindowExtUnix;
            use wry::WebViewBuilderExtUnix;

            let vbox = parent
                .default_vbox()
                .context("Tao window did not expose a GTK container")?;
            builder.build_gtk(vbox)?
        };

        #[cfg(not(target_os = "linux"))]
        let webview = builder.build(parent)?;

        Self::finish(webview, page_options, session)
    }

    /// Creates a positioned child WebView for a NativeUi region.
    #[cfg(not(target_os = "linux"))]
    pub(crate) fn new_child(
        parent: &Window,
        config: WebViewConfig,
        policy: RegionPolicy,
        ipc_handler: IpcMessageHandler,
        transparent: bool,
        bounds: wry::Rect,
    ) -> Result<Self> {
        let (builder, page_options, session) =
            Self::prepare_builder(config, policy, ipc_handler, transparent);
        let webview = builder.with_bounds(bounds).build_as_child(parent)?;
        Self::finish(webview, page_options, session)
    }

    /// Creates a positioned child WebView in a GTK fixed container.
    #[cfg(target_os = "linux")]
    pub(crate) fn new_gtk_child(
        parent: &gtk::Fixed,
        config: WebViewConfig,
        policy: RegionPolicy,
        ipc_handler: IpcMessageHandler,
        transparent: bool,
        bounds: wry::Rect,
    ) -> Result<Self> {
        use wry::WebViewBuilderExtUnix;

        let (builder, page_options, session) =
            Self::prepare_builder(config, policy, ipc_handler, transparent);
        let webview = builder.with_bounds(bounds).build_gtk(parent)?;
        Self::finish(webview, page_options, session)
    }

    fn prepare_builder(
        config: WebViewConfig,
        policy: RegionPolicy,
        ipc_handler: IpcMessageHandler,
        transparent: bool,
    ) -> (WebViewBuilder<'static>, PageOptions, Option<Session>) {
        let WebViewConfig {
            source,
            devtools,
            on_file_drop,
            page_options,
            session,
            download_policy,
            on_download,
            protocols,
            untrusted: _untrusted,
        } = config;
        let effective_devtools = devtools || page_options.devtools;
        let incognito = matches!(
            session.as_ref().map(Session::kind),
            Some(SessionKind::Ephemeral)
        );
        let mut builder = match source {
            PageSource::Url(url) => WebViewBuilder::new().with_url(url),
            PageSource::Html(html) => WebViewBuilder::new().with_html(html),
        }
        .with_initialization_script(ipc::bridge_script())
        .with_ipc_handler(move |request| {
            ipc_handler(request.body().clone());
        })
        .with_devtools(effective_devtools)
        .with_incognito(incognito)
        .with_transparent(transparent || policy.transparent)
        .with_navigation_handler(move |url| policy.navigation.allows(&url))
        .with_permission_handler(move |kind| permission_response(policy.permissions, kind));

        for script in policy.initialization_scripts {
            builder = builder.with_initialization_script(script);
        }

        if let Some(user_agent) = &page_options.user_agent {
            builder = builder.with_user_agent(user_agent);
        }

        for protocol in protocols {
            let scheme = protocol.scheme.clone();
            let handler = protocol.handler.clone();
            builder =
                builder.with_custom_protocol(scheme, move |_id, request| match handler(&request) {
                    Ok(response) => response.into_wry_response(),
                    Err(error) => {
                        tracing::warn!(%error, "custom protocol handler failed");
                        wry::http::Response::builder()
                            .status(500)
                            .header("Content-Type", "text/plain; charset=utf-8")
                            .body(std::borrow::Cow::Owned(error.to_string().into_bytes()))
                            .expect("valid custom protocol error response")
                    }
                });
        }

        if let Some(on_download) = on_download {
            let started_handler = on_download.clone();
            let policy = download_policy.clone();
            builder = builder.with_download_started_handler(move |url, path: &mut PathBuf| {
                let decision = policy
                    .as_ref()
                    .map(|policy| policy(&url, path))
                    .unwrap_or(DownloadPolicy::Allow);
                if let DownloadPolicy::SaveTo(destination) = &decision {
                    *path = destination.clone();
                }
                let allowed = decision.is_allowed();
                if allowed {
                    started_handler(DownloadEvent::Started {
                        url: url.clone(),
                        path: path.clone(),
                    });
                }
                allowed
            });

            let completed_handler = on_download;
            builder = builder.with_download_completed_handler(move |url, path, success| {
                completed_handler(DownloadEvent::Completed { url, path, success });
            });
        }

        if let Some(handler) = on_file_drop {
            builder = builder.with_drag_drop_handler(move |event| {
                if let Some(event) = convert_file_drop_event(event) {
                    handler(event);
                    true
                } else {
                    false
                }
            });
        }

        (builder, page_options, session)
    }

    fn finish(
        webview: wry::WebView,
        page_options: PageOptions,
        session: Option<Session>,
    ) -> Result<Self> {
        if page_options.zoom != 1.0 {
            webview
                .zoom(page_options.zoom)
                .context("failed to apply initial page zoom")?;
        }

        Ok(Self {
            webview,
            page_options,
            session,
        })
    }

    /// Returns a page-level handle for navigation and page APIs.
    pub fn page(&self) -> PageHandle<'_> {
        PageHandle::new(self)
    }

    /// Returns the initial page options.
    pub fn page_options(&self) -> &PageOptions {
        &self.page_options
    }

    /// Returns the logical session bound to this page.
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    pub(crate) fn webview_ref(&self) -> &wry::WebView {
        &self.webview
    }

    /// Positions the native view.
    ///
    /// `bounds` is in device pixels; wry's DPI-aware rectangle keeps physical
    /// values unchanged on Windows and divides them back to logical units
    /// where the platform backend expects them.
    pub(crate) fn set_bounds(&self, bounds: wry::Rect) -> Result<()> {
        self.webview
            .set_bounds(bounds)
            .context("failed to position child WebView")
    }

    /// Shows or hides the native view.
    pub(crate) fn set_visible(&self, visible: bool) -> Result<()> {
        self.webview
            .set_visible(visible)
            .context("failed to change child WebView visibility")
    }

    /// Moves keyboard focus into the native view.
    pub(crate) fn focus(&self) -> Result<()> {
        self.webview
            .focus()
            .context("failed to focus child WebView")
    }

    /// Moves keyboard focus back to the parent window.
    pub(crate) fn focus_parent(&self) -> Result<()> {
        self.webview
            .focus_parent()
            .context("failed to return focus from child WebView")
    }

    /// Returns the GTK widget backing this WebView.
    #[cfg(target_os = "linux")]
    pub(crate) fn gtk_widget(&self) -> gtk::Widget {
        use gtk::prelude::*;
        use wry::WebViewExtUnix;

        self.webview.webview().upcast()
    }

    /// Evaluates JavaScript in the current page.
    pub fn evaluate_script(&self, script: &str) -> Result<()> {
        self.webview
            .evaluate_script(script)
            .context("failed to evaluate WebView JavaScript")
    }

    /// Reloads the current page.
    pub fn reload(&self) -> Result<()> {
        self.webview.reload().context("failed to reload WebView")
    }

    /// Navigates the WebView to a URL.
    pub fn navigate(&self, url: &str) -> Result<()> {
        self.webview
            .load_url(url)
            .with_context(|| format!("failed to navigate WebView to `{url}`"))
    }

    /// Replaces the current document with an HTML string.
    pub fn load_html(&self, html: &str) -> Result<()> {
        self.webview
            .load_html(html)
            .context("failed to load HTML into WebView")
    }

    /// Opens the platform developer tools.
    ///
    /// Available in debug builds and when the `devtools` feature is enabled,
    /// matching the upstream `wry` gate.
    #[cfg(any(debug_assertions, feature = "devtools"))]
    pub fn open_devtools(&self) {
        self.webview.open_devtools();
    }

    /// Returns the current page URL.
    pub fn url(&self) -> Result<Option<String>> {
        Ok(Some(
            self.webview.url().context("failed to read WebView URL")?,
        ))
    }
}

fn permission_response(policy: PermissionPolicy, _kind: PermissionKind) -> PermissionResponse {
    match policy {
        PermissionPolicy::PlatformDefault => PermissionResponse::Default,
        PermissionPolicy::DenyAll => PermissionResponse::Deny,
        PermissionPolicy::AllowAll => PermissionResponse::Allow,
    }
}

/// Returns the lower-case host of an absolute URL.
fn host_of(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        return Some(rest[..end].to_ascii_lowercase());
    }
    let host = authority
        .split(':')
        .next()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

fn convert_file_drop_event(event: DragDropEvent) -> Option<FileDropEvent> {
    match event {
        DragDropEvent::Enter { paths, position } => Some(FileDropEvent {
            kind: FileDropKind::Entered,
            paths,
            position: position_to_logical(position),
        }),
        DragDropEvent::Over { position } => Some(FileDropEvent {
            kind: FileDropKind::Over,
            paths: Vec::new(),
            position: position_to_logical(position),
        }),
        DragDropEvent::Drop { paths, position } => Some(FileDropEvent {
            kind: FileDropKind::Dropped,
            paths,
            position: position_to_logical(position),
        }),
        DragDropEvent::Leave => Some(FileDropEvent {
            kind: FileDropKind::Cancelled,
            paths: Vec::new(),
            position: (0.0, 0.0),
        }),
        _ => None,
    }
}

fn position_to_logical(position: (i32, i32)) -> (f64, f64) {
    (f64::from(position.0), f64::from(position.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wry::dpi::{PhysicalPosition, PhysicalSize};

    #[test]
    fn navigation_policy_matches_hosts_and_subdomains() {
        let policy = NavigationPolicy::AllowHosts(vec!["example.com".to_string()]);

        assert!(policy.allows("https://example.com/docs"));
        assert!(policy.allows("https://docs.example.com/docs"));
        assert!(policy.allows("about:blank"));
        assert!(!policy.allows("https://evil-example.com/"));
        assert!(!policy.allows("https://example.com.evil.test/"));
        assert!(!policy.allows("file:///tmp/page.html"));
    }

    #[test]
    fn navigation_policy_handles_ports_userinfo_and_ipv6() {
        let policy = NavigationPolicy::AllowHosts(vec!["example.com".to_string()]);
        assert!(policy.allows("https://user@example.com:8443/x"));

        let ipv6 = NavigationPolicy::AllowHosts(vec!["::1".to_string()]);
        assert!(ipv6.allows("http://[::1]:8080/"));
    }

    #[test]
    fn deny_navigation_policy_blocks_every_target() {
        assert!(!NavigationPolicy::DenyAll.allows("about:blank"));
        assert!(!NavigationPolicy::DenyAll.allows("https://example.com/"));
        assert!(NavigationPolicy::AllowAll.allows("https://example.com/"));
    }

    #[test]
    fn permission_policy_maps_to_wry_responses() {
        assert_eq!(
            permission_response(PermissionPolicy::PlatformDefault, PermissionKind::Camera),
            PermissionResponse::Default
        );
        assert_eq!(
            permission_response(PermissionPolicy::DenyAll, PermissionKind::Microphone),
            PermissionResponse::Deny
        );
        assert_eq!(
            permission_response(PermissionPolicy::AllowAll, PermissionKind::Geolocation),
            PermissionResponse::Allow
        );
    }

    #[test]
    #[ignore = "needs a desktop display and the platform system WebView; run explicitly"]
    fn child_webview_accepts_bounds_and_visibility() {
        use tao::{event_loop::EventLoop, window::WindowBuilder};

        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_visible(false)
            .build(&event_loop)
            .expect("window");
        let rect = wry::Rect {
            position: PhysicalPosition::new(0, 0).into(),
            size: PhysicalSize::new(120, 80).into(),
        };
        let config = WebViewConfig {
            source: PageSource::Url("about:blank".to_string()),
            ..WebViewConfig::default()
        };
        let handler: IpcMessageHandler = Arc::new(|_| {});

        #[cfg(target_os = "linux")]
        let handle = {
            use gtk::prelude::*;
            use tao::platform::unix::WindowExtUnix;

            let vbox = window.default_vbox().expect("GTK container");
            let fixed = gtk::Fixed::new();
            vbox.pack_start(&fixed, true, true, 0);
            fixed.show_all();
            WebViewHandle::new_gtk_child(
                &fixed,
                config,
                RegionPolicy::default(),
                handler,
                false,
                rect,
            )
            .expect("webview")
        };

        #[cfg(not(target_os = "linux"))]
        let handle =
            WebViewHandle::new_child(
                &window,
                config,
                RegionPolicy::default(),
                handler,
                false,
                rect,
            )
            .expect("webview");

        handle
            .set_bounds(wry::Rect {
                position: PhysicalPosition::new(10, 10).into(),
                size: PhysicalSize::new(100, 60).into(),
            })
            .expect("bounds");
        handle.set_visible(false).expect("hide");
    }

    #[test]
    fn converts_drop_paths_and_position() {
        let event = convert_file_drop_event(DragDropEvent::Drop {
            paths: vec![PathBuf::from("/tmp/report.csv")],
            position: (42, 18),
        })
        .expect("drop event");

        assert_eq!(event.kind, FileDropKind::Dropped);
        assert_eq!(event.paths, vec![PathBuf::from("/tmp/report.csv")]);
        assert_eq!(event.position, (42.0, 18.0));
    }

    #[test]
    fn converts_leave_to_cancelled() {
        let event = convert_file_drop_event(DragDropEvent::Leave).expect("cancel event");
        assert_eq!(event.kind, FileDropKind::Cancelled);
        assert!(event.paths.is_empty());
    }
}
