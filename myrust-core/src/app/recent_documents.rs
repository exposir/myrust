//! Recently opened documents and taskbar jump-list semantics.
//!
//! Recent documents are stored as JSON in the platform application data
//! directory. Windows JumpList publication is an interface-only boundary in
//! this batch: the installer and taskbar integration are deferred to Phase 4.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::window_state::JsonFileStore;

/// A recently opened document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentDocument {
    /// Absolute or caller-supplied path to the document.
    pub path: PathBuf,
    /// Unix timestamp in seconds when the document was most recently added.
    pub added_at: u64,
    /// Human-readable name shown in menus and jump lists.
    pub display_name: String,
}

/// Persistent recently opened document manager.
pub struct RecentDocuments {
    store: JsonFileStore,
    limit: usize,
}

impl RecentDocuments {
    /// Creates a recent-document store at `<data dir>/<app_id>/recent-documents.json`.
    pub fn new(app_id: &str) -> Result<Self> {
        let data_dir =
            dirs::data_dir().context("platform application data directory is unavailable")?;
        Ok(Self::at_path(
            data_dir.join(app_id).join("recent-documents.json"),
            20,
        ))
    }

    /// Creates a store at an explicit path with a maximum retained entry count.
    pub fn at_path(path: impl Into<PathBuf>, limit: usize) -> Self {
        Self {
            store: JsonFileStore::at_path(path),
            limit: limit.max(1),
        }
    }

    /// Returns the JSON file used by this manager.
    pub fn path(&self) -> &Path {
        self.store.path()
    }

    /// Adds or refreshes a document path.
    ///
    /// Missing paths are skipped and return `Ok(false)`. Duplicate paths are
    /// removed before the refreshed entry is inserted at the front.
    pub fn add(&self, path: impl AsRef<Path>) -> Result<bool> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(false);
        }

        let mut documents = self.read()?;
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        documents.retain(|document| document.path != path && document.path != canonical);
        documents.insert(
            0,
            RecentDocument {
                path: canonical.clone(),
                added_at: unix_timestamp(),
                display_name: display_name(path),
            },
        );
        documents.truncate(self.limit);
        self.write(&documents)?;
        Ok(true)
    }

    /// Lists recent documents, newest first, skipping paths that no longer exist.
    pub fn list(&self) -> Result<Vec<RecentDocument>> {
        let mut documents = self.read()?;
        let before = documents.len();
        documents.retain(|document| document.path.exists());
        if documents.len() != before {
            self.write(&documents)?;
        }
        Ok(documents)
    }

    /// Removes one path from the recent-document list.
    pub fn remove(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut documents = self.read()?;
        documents.retain(|document| document.path != path && document.path != canonical);
        self.write(&documents)
    }

    /// Clears all recent documents.
    pub fn clear(&self) -> Result<()> {
        self.write(&[])
    }

    /// Returns the configured maximum number of entries.
    pub fn limit(&self) -> usize {
        self.limit
    }

    fn read(&self) -> Result<Vec<RecentDocument>> {
        let file: RecentDocumentsFile = self.store.read_json()?;
        Ok(file.documents)
    }

    fn write(&self, documents: &[RecentDocument]) -> Result<()> {
        let file = RecentDocumentsFile {
            version: 1,
            documents: documents.to_vec(),
        };
        self.store.write_json(&file)
    }
}

/// Platform taskbar jump-list operations.
///
/// This trait exists so Windows packaging can provide a real implementation in
/// Phase 4. No platform implementation is registered in batch C.
pub trait JumpList {
    /// Replaces the recent-documents category in the platform jump list.
    fn set_recent_documents(&self, documents: &[RecentDocument]) -> Result<()>;
}

/// Placeholder jump-list backend that reports the capability as unavailable.
///
/// It does not silently succeed: callers can detect that jump-list publication
/// has not been implemented for the current platform/build.
#[derive(Debug, Default)]
pub struct UnsupportedJumpList;

impl JumpList for UnsupportedJumpList {
    fn set_recent_documents(&self, _documents: &[RecentDocument]) -> Result<()> {
        bail!(
            "JumpList publication is not implemented in Phase 2 batch C; it requires Windows installer/taskbar integration"
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecentDocumentsFile {
    version: u32,
    #[serde(default)]
    documents: Vec<RecentDocument>,
}

impl Default for RecentDocumentsFile {
    fn default() -> Self {
        Self {
            version: 1,
            documents: Vec::new(),
        }
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Deduplicates paths while preserving their original order.
pub fn deduplicate_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn recent_documents_deduplicate_and_skip_missing_paths() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "myrust-recent-docs-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let existing = dir.join("notes.md");
        std::fs::write(&existing, "hello").expect("write document");
        let store = RecentDocuments::at_path(dir.join("recent.json"), 2);

        assert!(!store.add(dir.join("missing.md")).expect("skip missing"));
        assert!(store.add(&existing).expect("add existing"));
        assert!(store.add(&existing).expect("refresh existing"));
        let documents = store.list().expect("list documents");
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].display_name, "notes.md");

        store.remove(&existing).expect("remove document");
        assert!(store.list().expect("list empty").is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
