//! Wry-based system WebView wrapper.
//!
//! The wrapper uses WKWebView on macOS, WebView2 on Windows, and WebKitGTK on
//! Linux. A JavaScript IPC bridge is injected before application scripts run.

use std::sync::Arc;

use anyhow::{Context, Result};
use tao::window::Window;
use wry::WebViewBuilder;

use crate::ipc;

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
#[derive(Debug, Clone, Default)]
pub struct WebViewConfig {
    /// Initial page URL or embedded HTML.
    pub source: PageSource,
    /// Enables platform developer tools where available.
    pub devtools: bool,
}

/// Callback invoked with the raw JSON body sent by `window.ipc.postMessage`.
pub(crate) type IpcMessageHandler = Arc<dyn Fn(String) + Send + Sync + 'static>;

/// Native WebView owned by a managed application window.
pub struct WebViewHandle {
    webview: wry::WebView,
}

impl WebViewHandle {
    /// Creates a WebView inside a Tao window.
    pub(crate) fn new(
        parent: &Window,
        config: WebViewConfig,
        ipc_handler: IpcMessageHandler,
    ) -> Result<Self> {
        let builder = match config.source {
            PageSource::Url(url) => WebViewBuilder::new().with_url(url),
            PageSource::Html(html) => WebViewBuilder::new().with_html(html),
        }
        .with_initialization_script(ipc::bridge_script())
        .with_ipc_handler(move |request| {
            ipc_handler(request.body().clone());
        })
        .with_devtools(config.devtools);

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

        Ok(Self { webview })
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
