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
/// Wry-based WebView wrapper.
pub mod webview;

pub use app::{App, AppContext, WindowConfig, WindowId, WindowInfo};
pub use webview::{PageSource, WebViewConfig, WebViewHandle};
