//! Page-level WebView operations.
//!
//! [`PageHandle`] gathers navigation, script execution, zoom, find-in-page,
//! session access, and content-rule metadata behind one handle.

use anyhow::{Context, Result};

use super::{
    session::{ScrollPosition, Session, SessionState},
    WebViewHandle,
};

/// Native page zoom, developer-tool, user-agent, find, and rule options.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PageOptions {
    /// Initial zoom level. `1.0` is the native default.
    pub zoom: f64,
    /// Enables developer tools where supported.
    pub devtools: bool,
    /// Optional initial WebView user agent.
    pub user_agent: Option<String>,
    /// Default options for in-page find operations.
    pub find: Option<FindOptions>,
    /// WebKit-style content-rule metadata.
    ///
    /// Rules are stored but not applied by the current Wry backend.
    pub content_rules: Vec<ContentRule>,
}

impl Default for PageOptions {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            devtools: false,
            user_agent: None,
            find: None,
            content_rules: Vec::new(),
        }
    }
}

/// Browser in-page find flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FindOptions {
    /// Match letter case.
    pub match_case: bool,
    /// Search backward from the current selection.
    pub backwards: bool,
    /// Wrap around the beginning or end of the document.
    pub wrap_around: bool,
    /// Match whole words only.
    pub whole_word: bool,
    /// Search child frames as well as the main frame.
    pub search_in_frames: bool,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            match_case: false,
            backwards: false,
            wrap_around: true,
            whole_word: false,
            search_in_frames: true,
        }
    }
}

/// WebKit content-rule resource categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ContentResourceType {
    /// Documents and frames.
    Document,
    /// Images.
    Image,
    /// Stylesheets.
    StyleSheet,
    /// Scripts.
    Script,
    /// Fonts.
    Font,
    /// Audio and video.
    Media,
    /// Fetch and XMLHttpRequest traffic.
    Raw,
    /// Any resource type.
    Other,
}

/// WebKit content-rule load phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ContentLoadType {
    /// Top-level document load.
    TopFrame,
    /// Subframe or resource load.
    ChildFrame,
    /// Third-party load.
    ThirdParty,
}

/// WebKit-style content-rule trigger.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContentRuleTrigger {
    /// URL filter expression.
    pub url_filter: String,
    /// Whether URL matching is case-sensitive.
    pub url_filter_is_case_sensitive: bool,
    /// Restrict the rule to these resource types.
    pub resource_types: Vec<ContentResourceType>,
    /// Restrict the rule to these load phases.
    pub load_types: Vec<ContentLoadType>,
    /// Apply only when the document is hosted on these domains.
    pub if_domains: Vec<String>,
    /// Skip when the document is hosted on these domains.
    pub unless_domains: Vec<String>,
    /// Apply only when the top document URL matches these filters.
    pub if_top_urls: Vec<String>,
    /// Skip when the top document URL matches these filters.
    pub unless_top_urls: Vec<String>,
}

impl Default for ContentRuleTrigger {
    fn default() -> Self {
        Self {
            url_filter: "*".to_string(),
            url_filter_is_case_sensitive: false,
            resource_types: Vec::new(),
            load_types: Vec::new(),
            if_domains: Vec::new(),
            unless_domains: Vec::new(),
            if_top_urls: Vec::new(),
            unless_top_urls: Vec::new(),
        }
    }
}

/// WebKit-style content-rule action.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContentRuleAction {
    /// Block matching resources.
    pub block: bool,
    /// Ignore all preceding rules for this load.
    pub ignore_previous_rules: bool,
    /// Hide matching elements with `display: none`.
    pub css_display_none: bool,
    /// Upgrade matching URLs to HTTPS.
    pub make_https_only: bool,
}

/// Declarative WebKit content rule.
///
/// Wry does not currently expose WebKit content-rule compilation. These
/// structures are retained for future backend integration and are deliberately
/// not treated as active filtering rules.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContentRule {
    /// Conditions that select resources.
    pub trigger: ContentRuleTrigger,
    /// Action applied to selected resources.
    pub action: ContentRuleAction,
}

/// Borrowed page handle obtained from [`crate::AppContext::page`].
#[derive(Clone, Copy)]
pub struct PageHandle<'a> {
    webview: &'a WebViewHandle,
}

impl<'a> PageHandle<'a> {
    pub(crate) fn new(webview: &'a WebViewHandle) -> Self {
        Self { webview }
    }

    /// Returns the current page URL.
    pub fn url(&self) -> Result<Option<String>> {
        self.webview.url()
    }

    /// Reloads the current page.
    pub fn reload(&self) -> Result<()> {
        self.webview.reload()
    }

    /// Navigates to a URL.
    pub fn navigate(&self, url: &str) -> Result<()> {
        self.webview.navigate(url)
    }

    /// Evaluates JavaScript in the current page.
    pub fn evaluate_script(&self, script: &str) -> Result<()> {
        self.webview.evaluate_script(script)
    }

    /// Loads an HTML document into the WebView.
    pub fn load_html(&self, html: &str) -> Result<()> {
        self.webview.load_html(html)
    }

    /// Sets the page zoom level.
    pub fn set_zoom(&self, zoom: f64) -> Result<()> {
        self.webview
            .webview_ref()
            .zoom(zoom)
            .context("failed to set page zoom")
    }

    /// Opens platform developer tools.
    pub fn open_devtools(&self) {
        self.webview.open_devtools();
    }

    /// Returns initial page options.
    pub fn options(&self) -> &PageOptions {
        self.webview.page_options()
    }

    /// Returns the optional logical session bound to this WebView.
    pub fn session(&self) -> Option<&Session> {
        self.webview.session()
    }

    /// Searches the current page using the configured find defaults.
    pub fn find_text(&self, query: &str) -> Result<()> {
        let options = self.options().find.unwrap_or_default();
        self.find_text_with(query, options)
    }

    /// Searches the current page with explicit find options.
    pub fn find_text_with(&self, query: &str, options: FindOptions) -> Result<()> {
        let query = serde_json::to_string(query).context("failed to encode find query")?;
        let script = format!(
            "window.find({query}, {}, {}, {}, {}, {}, false);",
            options.match_case,
            options.backwards,
            options.wrap_around,
            options.whole_word,
            options.search_in_frames
        );
        self.evaluate_script(&script)
    }

    /// Clears the current text selection used by find-in-page.
    pub fn stop_find(&self) -> Result<()> {
        self.evaluate_script("window.getSelection && window.getSelection().removeAllRanges();")
    }

    /// Restores a page-session navigation target and scroll offset.
    ///
    /// Wry does not yet expose back/forward history enumeration, so callers
    /// persist [`SessionState`] through a [`super::SessionStateStore`] and pass
    /// it here when reopening a page.
    pub fn restore_session_state(&self, state: &SessionState) -> Result<()> {
        if let Some(entry) = state.history.get(state.current_index) {
            self.navigate(&entry.url)?;
        }
        self.restore_scroll(state.scroll_position)
    }

    /// Scrolls the current page to a saved CSS-pixel offset.
    pub fn restore_scroll(&self, position: ScrollPosition) -> Result<()> {
        let script = format!(
            "window.scrollTo({{ left: {}, top: {}, behavior: \"auto\" }});",
            position.x, position.y
        );
        self.evaluate_script(&script)
    }

    /// Returns configured WebKit-style content rules.
    pub fn content_rules(&self) -> &[ContentRule] {
        &self.options().content_rules
    }
}
