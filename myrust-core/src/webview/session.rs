//! Logical WebView session identity and state hooks.
//!
//! Wry 0.57 does not expose a complete webview partition API on every
//! platform. This batch establishes stable logical session ids and data
//! directories, but persistent sessions are not yet storage-isolated from one
//! another. Ephemeral sessions request Wry incognito mode. Do not treat
//! [`Session::is_storage_isolated`] as true until partition support is wired.
//!
//! Wry also does not expose a public back/forward history list or scroll
//! position API. [`SessionStateStore`] defines the persistence hook; callers
//! must collect history and scroll data once a page backend can provide it.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Session lifetime and identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionKind {
    /// A named session expected to survive process restarts.
    Persistent(String),
    /// A process-local, incognito-style session.
    Ephemeral,
}

/// Logical session bound to a WebView.
#[derive(Debug, Clone)]
pub struct Session {
    kind: SessionKind,
    identifier: String,
    data_directory: PathBuf,
}

impl Session {
    /// Creates the session identity and its data directory.
    ///
    /// Persistent sessions live below the platform application data directory.
    /// Ephemeral sessions use the platform temporary directory.
    pub fn new(app_id: &str, kind: SessionKind) -> Result<Self> {
        static EPHEMERAL_COUNTER: AtomicU64 = AtomicU64::new(1);

        let app_id = sanitize_component(app_id);
        let (identifier, root) = match &kind {
            SessionKind::Persistent(name) => {
                let identifier = sanitize_component(name);
                let root = dirs::data_dir()
                    .context("platform application data directory is unavailable")?
                    .join(&app_id)
                    .join("sessions");
                (identifier, root)
            }
            SessionKind::Ephemeral => {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|duration| duration.as_nanos())
                    .unwrap_or_default();
                let counter = EPHEMERAL_COUNTER.fetch_add(1, Ordering::Relaxed);
                let identifier = format!("ephemeral-{}-{timestamp}-{counter}", std::process::id());
                let root = std::env::temp_dir()
                    .join("myrust")
                    .join(&app_id)
                    .join("ephemeral-sessions");
                (identifier, root)
            }
        };
        let data_directory = root.join(&identifier);
        fs::create_dir_all(&data_directory).with_context(|| {
            format!(
                "failed to create session directory {}",
                data_directory.display()
            )
        })?;

        Ok(Self {
            kind,
            identifier,
            data_directory,
        })
    }

    /// Returns the session kind.
    pub fn kind(&self) -> &SessionKind {
        &self.kind
    }

    /// Returns the filesystem-safe session identifier.
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Returns the directory reserved for this session.
    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }

    /// Returns whether persistent storage is actually partitioned per session.
    ///
    /// This is currently always `false`: Wry is not configured with a
    /// per-session `WebContext` in Phase 2 batch B. Ephemeral sessions may still
    /// run in Wry's incognito mode.
    pub fn is_storage_isolated(&self) -> bool {
        false
    }
}

/// One saved navigation entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionHistoryEntry {
    /// Page URL.
    pub url: String,
    /// Optional document title.
    pub title: Option<String>,
}

/// Scroll offset in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScrollPosition {
    /// Horizontal scroll offset.
    pub x: f64,
    /// Vertical scroll offset.
    pub y: f64,
}

/// Persisted page-session state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    /// Back/forward history entries.
    pub history: Vec<SessionHistoryEntry>,
    /// Index of the currently active history entry.
    pub current_index: usize,
    /// Last known page scroll position.
    pub scroll_position: ScrollPosition,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            current_index: 0,
            scroll_position: ScrollPosition { x: 0.0, y: 0.0 },
        }
    }
}

/// Persistence hook for page-session navigation and scroll state.
pub trait SessionStateStore: Send + Sync {
    /// Loads saved state for a session identifier.
    fn load(&self, session_id: &str) -> Result<Option<SessionState>>;

    /// Saves state for a session identifier.
    fn save(&self, session_id: &str, state: &SessionState) -> Result<()>;
}

/// In-memory [`SessionStateStore`] useful for tests and non-persistent apps.
#[derive(Default)]
pub struct MemorySessionStateStore {
    states: Mutex<HashMap<String, SessionState>>,
}

impl SessionStateStore for MemorySessionStateStore {
    fn load(&self, session_id: &str) -> Result<Option<SessionState>> {
        Ok(self
            .states
            .lock()
            .expect("session state lock poisoned")
            .get(session_id)
            .cloned())
    }

    fn save(&self, session_id: &str, state: &SessionState) -> Result<()> {
        self.states
            .lock()
            .expect("session state lock poisoned")
            .insert(session_id.to_string(), state.clone());
        Ok(())
    }
}

fn sanitize_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "default".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_components_are_filesystem_safe() {
        assert_eq!(sanitize_component("my app/one"), "my-app-one");
        assert_eq!(sanitize_component(""), "default");
    }

    #[test]
    fn memory_store_round_trips_session_state() {
        let store = MemorySessionStateStore::default();
        let state = SessionState {
            history: vec![SessionHistoryEntry {
                url: "https://example.com".to_string(),
                title: Some("Example".to_string()),
            }],
            current_index: 0,
            scroll_position: ScrollPosition { x: 0.0, y: 120.0 },
        };
        store.save("main", &state).expect("save state");
        assert_eq!(store.load("main").expect("load state"), Some(state));
    }
}
