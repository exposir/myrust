//! Core desktop runtime for myrust.
//!
//! The crate provides a single application object, native WebView windows,
//! typed IPC, and a small set of cross-platform desktop APIs.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

/// Application lifecycle and native window management.
pub mod app;
/// Native desktop integrations.
pub mod desktop;
/// Typed inter-process communication between Rust and WebView JavaScript.
pub mod ipc;
/// Release, distribution, signing, and update support.
pub mod release;
/// Wry-based WebView wrapper.
pub mod webview;

pub use app::file_watcher::{
    FileEvent, FileEventDetail, FileEventKind, FileWatcher, WatchHandle, WatchOptions,
};
pub use app::recent_documents::{JumpList, RecentDocument, RecentDocuments, UnsupportedJumpList};
pub use app::window_state::{JsonFileStore, WindowState, WindowStateStore};
pub use app::{App, AppContext, Rect, TitleButtons, Vibrancy, WindowConfig, WindowId, WindowInfo};
pub use desktop::{
    Activation, CustomProtocol, DownloadEvent, DownloadHandler, DownloadPolicy,
    DownloadPolicyHandler, ProtocolHandler, ProtocolResponse, SingleInstance,
};
pub use release::{
    HttpUpdateSource, LocalUpdateSource, PackageConfig, PackageOutput, Packager, SignatureVerifier,
    SigningConfig, SigningPlatform, UnimplementedSignatureVerifier, UpdateInfo, UpdateSource,
    Updater,
};
pub use webview::{
    ContentLoadType, ContentResourceType, ContentRule, ContentRuleAction, ContentRuleTrigger,
    FileDropEvent, FileDropHandler, FileDropKind, FindOptions, MemorySessionStateStore, PageHandle,
    PageOptions, PageSource, ScrollPosition, Session, SessionHistoryEntry, SessionKind,
    SessionState, SessionStateStore, WebViewConfig, WebViewHandle,
};
