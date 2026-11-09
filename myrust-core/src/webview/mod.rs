//! Wry-based system WebView wrapper.
//!
//! The wrapper uses WKWebView on macOS, WebView2 on Windows, and WebKitGTK on
//! Linux. A JavaScript IPC bridge is injected before application scripts run.

use std::{fmt, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tao::window::Window;
use wry::{DragDropEvent, WebViewBuilder};

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
        .with_transparent(transparent);

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
