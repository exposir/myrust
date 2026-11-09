//! Global keyboard shortcuts built on `global-hotkey`.

use anyhow::{Context, Result};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager};
use serde::{Deserialize, Serialize};

/// Global shortcut state event.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ShortcutEvent {
    /// Stable hotkey id returned by [`GlobalShortcutManager::register`].
    pub id: u32,
    /// Whether the shortcut was pressed or released.
    pub pressed: bool,
}

/// Manages registration and polling for process-wide keyboard shortcuts.
pub struct GlobalShortcutManager {
    manager: GlobalHotKeyManager,
}

impl GlobalShortcutManager {
    /// Creates the platform global shortcut manager.
    pub fn new() -> Result<Self> {
        Ok(Self {
            manager: GlobalHotKeyManager::new()
                .context("failed to initialize global shortcut manager")?,
        })
    }

    /// Registers a shortcut such as `"shift+alt+KeyD"` and returns its id.
    pub fn register(&self, shortcut: &str) -> Result<u32> {
        let hotkey = parse_hotkey(shortcut)?;
        self.manager
            .register(hotkey)
            .with_context(|| format!("failed to register shortcut `{shortcut}`"))?;
        Ok(hotkey.id())
    }

    /// Unregisters a previously registered shortcut.
    pub fn unregister(&self, shortcut: &str) -> Result<()> {
        let hotkey = parse_hotkey(shortcut)?;
        self.manager
            .unregister(hotkey)
            .with_context(|| format!("failed to unregister shortcut `{shortcut}`"))
    }

    /// Polls one pending global shortcut event without blocking.
    pub fn poll_event(&self) -> Option<ShortcutEvent> {
        GlobalHotKeyEvent::receiver()
            .try_recv()
            .ok()
            .map(|event| ShortcutEvent {
                id: event.id(),
                pressed: matches!(event.state(), global_hotkey::HotKeyState::Pressed),
            })
    }
}

fn parse_hotkey(shortcut: &str) -> Result<HotKey> {
    shortcut
        .parse::<HotKey>()
        .with_context(|| format!("invalid global shortcut `{shortcut}`"))
}
