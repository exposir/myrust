//! WebView download policy and events.
//!
//! Wry 0.57 exposes download start and completion callbacks. It does not expose
//! byte progress, so [`DownloadEvent::Progress`] is defined for API stability
//! but is never emitted by the current backend.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Decision returned when a WebView download starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadPolicy {
    /// Allow the download at the suggested path.
    Allow,
    /// Deny the download.
    Deny,
    /// Allow the download at a caller-provided path.
    SaveTo(PathBuf),
}

impl DownloadPolicy {
    /// Returns whether the policy allows the download.
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Self::Deny)
    }
}

/// Download lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadEvent {
    /// Download is starting. The path is the current suggested destination.
    Started {
        /// Source URL.
        url: String,
        /// Suggested destination path.
        path: PathBuf,
    },
    /// Byte-level progress.
    ///
    /// **Not emitted by the current wry backend.** It is present so adding a
    /// progress-capable backend does not break the public event type.
    Progress {
        /// Source URL.
        url: String,
        /// Bytes received.
        received: u64,
        /// Total bytes when known.
        total: Option<u64>,
    },
    /// Download completed or failed.
    Completed {
        /// Source URL.
        url: String,
        /// Destination path when the backend reports it.
        path: Option<PathBuf>,
        /// Whether the download succeeded.
        success: bool,
    },
}

/// Callback invoked for download events.
pub type DownloadHandler = std::sync::Arc<dyn Fn(DownloadEvent) + Send + Sync + 'static>;

/// Callback that chooses whether and where a download starts.
pub type DownloadPolicyHandler =
    std::sync::Arc<dyn Fn(&str, &PathBuf) -> DownloadPolicy + Send + Sync + 'static>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_reports_allow_and_deny() {
        assert!(DownloadPolicy::Allow.is_allowed());
        assert!(DownloadPolicy::SaveTo(PathBuf::from("/tmp/file")).is_allowed());
        assert!(!DownloadPolicy::Deny.is_allowed());
    }
}
