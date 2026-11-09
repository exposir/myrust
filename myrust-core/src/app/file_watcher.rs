//! Public event-driven filesystem watcher.
//!
//! This API wraps `notify`. It preserves notify's rename trackers when the
//! backend supplies them, which helps distinguish a rename-over replacement
//! from an ordinary write. It does not inspect filesystem inode numbers and
//! therefore cannot prove that an inode was reused; callers requiring that
//! guarantee must compare platform file identities themselves.

use std::{
    path::{Path, PathBuf},
    sync::mpsc::{channel, Receiver, RecvTimeoutError},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

/// Coarse filesystem event category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileEventKind {
    /// A path was created.
    Created,
    /// A path was modified.
    Modified,
    /// A path was removed.
    Removed,
    /// A path was renamed.
    Renamed,
    /// The backend could not classify the change.
    Other,
}

/// Additional detail emitted when [`WatchOptions::hints_only`] is `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileEventDetail {
    /// File contents or metadata changed.
    Data,
    /// Metadata changed.
    Metadata,
    /// Rename or move event.
    Rename,
    /// Other backend-specific modification.
    Other,
}

/// Filesystem event delivered to a watcher consumer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEvent {
    /// Coarse event kind.
    pub kind: FileEventKind,
    /// Paths affected by the event. Rename pairs contain source then target.
    pub paths: Vec<PathBuf>,
    /// Backend-provided rename tracker, when available.
    pub tracker: Option<usize>,
    /// Detailed modification category when hints-only mode is disabled.
    pub detail: Option<FileEventDetail>,
}

/// File-watcher behavior options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchOptions {
    /// Watch subdirectories as well.
    pub recursive: bool,
    /// Emit coarse change hints only.
    ///
    /// Access events are always dropped. In hints-only mode [`FileEvent::kind`]
    /// is still populated, but [`FileEvent::detail`] is `None`.
    pub hints_only: bool,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            hints_only: true,
        }
    }
}

/// A live filesystem watch. Dropping this handle stops delivery.
pub struct WatchHandle {
    _watcher: RecommendedWatcher,
    events: Receiver<FileEvent>,
}

impl WatchHandle {
    /// Blocks up to `timeout` for the next event.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<FileEvent>> {
        match self.events.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Ok(None),
        }
    }

    /// Blocks until the next event or watcher shutdown.
    pub fn recv(&self) -> Result<Option<FileEvent>> {
        match self.events.recv() {
            Ok(event) => Ok(Some(event)),
            Err(_) => Ok(None),
        }
    }
}

/// Event-driven filesystem watcher factory.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileWatcher;

impl FileWatcher {
    /// Watches a path with default options.
    pub fn watch(path: impl AsRef<Path>) -> Result<WatchHandle> {
        Self::watch_with(path, WatchOptions::default())
    }

    /// Watches a path with explicit options.
    pub fn watch_with(path: impl AsRef<Path>, options: WatchOptions) -> Result<WatchHandle> {
        let path = path.as_ref();
        if !path.exists() {
            bail!("watch path does not exist: {}", path.display());
        }

        let (sender, receiver) = channel();

        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                let Ok(event) = result else {
                    return;
                };
                let Some(event) = convert_event(event, options.hints_only) else {
                    return;
                };
                let _ = sender.send(event);
            })
            .context("failed to create native filesystem watcher")?;
        let mode = if options.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        watcher
            .watch(path, mode)
            .with_context(|| format!("failed to watch {}", path.display()))?;

        Ok(WatchHandle {
            _watcher: watcher,
            events: receiver,
        })
    }
}

fn convert_event(event: notify::Event, hints_only: bool) -> Option<FileEvent> {
    if matches!(event.kind, EventKind::Access(_)) {
        return None;
    }

    let kind = match &event.kind {
        EventKind::Create(_) => FileEventKind::Created,
        EventKind::Remove(_) => FileEventKind::Removed,
        EventKind::Modify(notify::event::ModifyKind::Name(_)) => FileEventKind::Renamed,
        EventKind::Modify(_) => FileEventKind::Modified,
        EventKind::Any | EventKind::Other => FileEventKind::Other,
        _ => FileEventKind::Other,
    };
    let detail = if hints_only {
        None
    } else {
        match event.kind {
            EventKind::Modify(notify::event::ModifyKind::Data(_)) => Some(FileEventDetail::Data),
            EventKind::Modify(notify::event::ModifyKind::Metadata(_)) => {
                Some(FileEventDetail::Metadata)
            }
            EventKind::Modify(notify::event::ModifyKind::Name(_)) => Some(FileEventDetail::Rename),
            EventKind::Modify(_) => Some(FileEventDetail::Other),
            _ => None,
        }
    };

    Some(FileEvent {
        kind,
        paths: event.paths,
        tracker: event.attrs.tracker(),
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_rename_and_ignore_access() {
        let event = notify::Event::new(EventKind::Modify(notify::event::ModifyKind::Name(
            notify::event::RenameMode::Both,
        )))
        .add_path(PathBuf::from("/tmp/a"))
        .add_path(PathBuf::from("/tmp/b"))
        .set_tracker(7);
        let converted = convert_event(event, true).expect("rename event");
        assert_eq!(converted.kind, FileEventKind::Renamed);
        assert_eq!(converted.paths.len(), 2);
        assert_eq!(converted.tracker, Some(7));
        assert_eq!(converted.detail, None);

        let access = notify::Event::new(EventKind::Access(notify::event::AccessKind::Read));
        assert!(convert_event(access, true).is_none());
    }
}
