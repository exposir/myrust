//! Recently opened documents and taskbar jump-list semantics.
//!
//! Recent documents are stored as JSON in the platform application data
//! directory. On Windows, [`WindowsJumpList`] publishes the recent-documents
//! category through the Shell COM API. Other platforms retain an explicit
//! unsupported backend instead of pretending publication succeeded.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::window_state::JsonFileStore;

#[cfg(any(target_os = "windows", test))]
const MAX_JUMP_LIST_ITEMS: usize = 10;

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
/// [`WindowsJumpList`] implements this trait on Windows. Callers on other
/// platforms can use [`UnsupportedJumpList`] to receive an explicit capability
/// error.
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
        bail!("JumpList publication is unavailable on this platform")
    }
}

/// Windows taskbar jump-list publisher for an application-defined AppUserModelID.
///
/// The AppUserModelID must be the same identifier used by the application's
/// installer and shortcut. The full API is available only on Windows.
#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
pub struct WindowsJumpList {
    app_id: String,
}

#[cfg(target_os = "windows")]
impl WindowsJumpList {
    /// Creates a publisher for the given application-defined AppUserModelID.
    pub fn new(app_id: impl Into<String>) -> Result<Self> {
        let app_id = app_id.into();
        validate_jump_list_app_id(&app_id)?;
        Ok(Self { app_id })
    }

    /// Returns the AppUserModelID used for publication.
    pub fn app_id(&self) -> &str {
        &self.app_id
    }
}

#[cfg(target_os = "windows")]
impl JumpList for WindowsJumpList {
    fn set_recent_documents(&self, documents: &[RecentDocument]) -> Result<()> {
        publish_windows_jump_list(&self.app_id, documents)
    }
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct JumpListEntry {
    path: PathBuf,
    title: String,
}

#[cfg(any(target_os = "windows", test))]
fn jump_list_entries(
    documents: &[RecentDocument],
    removed_paths: &[PathBuf],
) -> Vec<JumpListEntry> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();

    for document in documents {
        if entries.len() == MAX_JUMP_LIST_ITEMS {
            break;
        }
        if !document.path.is_absolute()
            || removed_paths.iter().any(|path| path == &document.path)
            || !seen.insert(document.path.clone())
        {
            continue;
        }

        let title = if document.display_name.trim().is_empty() {
            document
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .map_or_else(
                    || document.path.display().to_string(),
                    |name| name.to_string(),
                )
        } else {
            document.display_name.trim().to_string()
        };
        entries.push(JumpListEntry {
            path: document.path.clone(),
            title,
        });
    }

    entries
}

#[cfg(any(target_os = "windows", test))]
fn validate_jump_list_app_id(app_id: &str) -> Result<()> {
    if app_id.is_empty() {
        bail!("Windows AppUserModelID must not be empty");
    }
    if app_id.len() > 128 {
        bail!("Windows AppUserModelID must not exceed 128 bytes");
    }
    if app_id
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        bail!("Windows AppUserModelID must not contain whitespace or control characters");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
struct ComInitialization {
    uninitialize: bool,
}

#[cfg(target_os = "windows")]
impl ComInitialization {
    #[allow(unsafe_code)]
    fn apartment_threaded() -> Result<Self> {
        use windows::Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED},
        };

        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result.is_err() && result != RPC_E_CHANGED_MODE {
            result
                .ok()
                .context("failed to initialize COM for jump-list publication")?;
        }
        Ok(Self {
            uninitialize: result.is_ok(),
        })
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComInitialization {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if self.uninitialize {
            unsafe {
                windows::Win32::System::Com::CoUninitialize();
            }
        }
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn publish_windows_jump_list(app_id: &str, documents: &[RecentDocument]) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows::{
        core::{IUnknown, Interface, PCWSTR},
        Win32::{
            System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
            UI::Shell::{
                Common::{IObjectArray, IObjectCollection},
                DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellItem,
                SHCreateItemFromParsingName,
            },
        },
    };

    let _com = ComInitialization::apartment_threaded()?;
    let entries = jump_list_entries(documents, &[]);
    let app_id_wide = wide_string(app_id);
    let category_wide = wide_string("Recent");

    unsafe {
        let destinations: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)
                .context("failed to create the Windows destination list")?;
        destinations
            .SetAppID(PCWSTR(app_id_wide.as_ptr()))
            .context("failed to set the jump-list AppUserModelID")?;

        let mut minimum_slots = 0u32;
        let _removed: IObjectArray = destinations
            .BeginList(&mut minimum_slots)
            .context("failed to begin Windows jump-list update")?;

        let collection: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)
                .context("failed to create the Windows jump-list item collection")?;
        for entry in &entries {
            let path: Vec<u16> = entry
                .path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None)
                .with_context(|| {
                    format!(
                        "failed to create a jump-list item for {}",
                        entry.path.display()
                    )
                })?;
            collection
                .AddObject(&item.cast::<IUnknown>()?)
                .with_context(|| {
                    format!("failed to add {} to the jump list", entry.path.display())
                })?;
        }

        destinations
            .AppendCategory(PCWSTR(category_wide.as_ptr()), &*collection)
            .context("failed to append the recent-documents jump-list category")?;
        destinations
            .CommitList()
            .context("failed to commit the Windows jump list")?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
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

    #[test]
    fn jump_list_entries_filter_deduplicate_and_fill_titles() {
        let documents = vec![
            RecentDocument {
                path: PathBuf::from("/documents/one.txt"),
                added_at: 3,
                display_name: "One".to_string(),
            },
            RecentDocument {
                path: PathBuf::from("relative.txt"),
                added_at: 2,
                display_name: "Relative".to_string(),
            },
            RecentDocument {
                path: PathBuf::from("/documents/removed.txt"),
                added_at: 1,
                display_name: "Removed".to_string(),
            },
            RecentDocument {
                path: PathBuf::from("/documents/one.txt"),
                added_at: 0,
                display_name: "Duplicate".to_string(),
            },
            RecentDocument {
                path: PathBuf::from("/documents/two.txt"),
                added_at: 0,
                display_name: String::new(),
            },
        ];
        let removed = vec![PathBuf::from("/documents/removed.txt")];
        let entries = jump_list_entries(&documents, &removed);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, PathBuf::from("/documents/one.txt"));
        assert_eq!(entries[0].title, "One");
        assert_eq!(entries[1].path, PathBuf::from("/documents/two.txt"));
        assert_eq!(entries[1].title, "two.txt");
    }

    #[test]
    fn jump_list_caps_entries_and_validates_app_id() {
        let documents: Vec<_> = (0..12)
            .map(|index| RecentDocument {
                path: PathBuf::from(format!("/documents/{index}.txt")),
                added_at: index,
                display_name: format!("Document {index}"),
            })
            .collect();
        let entries = jump_list_entries(&documents, &[]);
        assert_eq!(entries.len(), MAX_JUMP_LIST_ITEMS);
        assert_eq!(entries[0].path, PathBuf::from("/documents/0.txt"));

        assert!(validate_jump_list_app_id("org.example.MyApp").is_ok());
        assert!(validate_jump_list_app_id("").is_err());
        assert!(validate_jump_list_app_id("org.example app").is_err());
        assert!(validate_jump_list_app_id(&"x".repeat(129)).is_err());
    }

    #[test]
    fn unsupported_jump_list_reports_an_explicit_error() {
        let error = UnsupportedJumpList
            .set_recent_documents(&[])
            .expect_err("unsupported backend must not silently succeed");
        assert!(error.to_string().contains("unavailable"));
    }
}
