//! Window geometry persistence.
//!
//! Coordinates and sizes are stored in logical pixels so the same state can be
//! restored across displays with different scale factors.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Persisted native window geometry and display state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// Logical x coordinate in the virtual desktop.
    pub x: f64,
    /// Logical y coordinate in the virtual desktop.
    pub y: f64,
    /// Logical inner width.
    pub width: f64,
    /// Logical inner height.
    pub height: f64,
    /// Whether the window was maximized.
    pub maximized: bool,
    /// Whether the window was full screen.
    pub fullscreen: bool,
    /// Backing scale factor of the display that held the window.
    pub display_scale: f64,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1024.0,
            height: 720.0,
            maximized: false,
            fullscreen: false,
            display_scale: 1.0,
        }
    }
}

/// Storage backend for named window state.
pub trait WindowStateStore: Send + Sync {
    /// Loads state for an application and stable window key.
    fn load(&self, app_id: &str, window_key: &str) -> Result<Option<WindowState>>;

    /// Saves state for an application and stable window key.
    fn save(&self, app_id: &str, window_key: &str, state: &WindowState) -> Result<()>;
}

/// JSON file store rooted in the platform's application data directory.
pub struct JsonFileStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl JsonFileStore {
    /// Creates a store at `<platform data dir>/<app_id>/window-state.json`.
    pub fn new(app_id: &str) -> Result<Self> {
        let data_dir =
            dirs::data_dir().context("platform application data directory is unavailable")?;
        let path = data_dir.join(app_id).join("window-state.json");
        Ok(Self::at_path(path))
    }

    /// Creates a store at an explicit path, useful for tests and custom app data.
    pub fn at_path(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }

    /// Returns the backing JSON file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads and deserializes the JSON file.
    ///
    /// A missing file yields `T::default()`. This lets other app-data stores
    /// reuse the same locked JSON persistence implementation.
    pub fn read_json<T>(&self) -> Result<T>
    where
        T: DeserializeOwned + Default,
    {
        let _guard = self.lock.lock().expect("window state store lock poisoned");
        self.read_json_unlocked()
    }

    fn read_json_unlocked<T>(&self) -> Result<T>
    where
        T: DeserializeOwned + Default,
    {
        if !self.path.exists() {
            return Ok(T::default());
        }
        let text = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("invalid JSON in {}", self.path.display()))
    }

    /// Serializes and writes the JSON file.
    pub fn write_json<T: Serialize>(&self, value: &T) -> Result<()> {
        let _guard = self.lock.lock().expect("window state store lock poisoned");
        self.write_json_unlocked(value)
    }

    fn write_json_unlocked<T: Serialize>(&self, value: &T) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(value)
            .context("failed to serialize JSON application data")?;
        fs::write(&self.path, text)
            .with_context(|| format!("failed to write {}", self.path.display()))
    }

    fn read_file(&self) -> Result<WindowStateFile> {
        self.read_json_unlocked()
    }

    fn write_file(&self, file: &WindowStateFile) -> Result<()> {
        self.write_json_unlocked(file)
    }
}

impl WindowStateStore for JsonFileStore {
    fn load(&self, app_id: &str, window_key: &str) -> Result<Option<WindowState>> {
        let _guard = self.lock.lock().expect("window state store lock poisoned");
        let file = self.read_file()?;
        Ok(file
            .apps
            .get(app_id)
            .and_then(|windows| windows.get(window_key))
            .cloned())
    }

    fn save(&self, app_id: &str, window_key: &str, state: &WindowState) -> Result<()> {
        let _guard = self.lock.lock().expect("window state store lock poisoned");
        let mut file = self.read_file()?;
        file.apps
            .entry(app_id.to_string())
            .or_default()
            .insert(window_key.to_string(), state.clone());
        self.write_file(&file)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WindowStateFile {
    version: u32,
    #[serde(default)]
    apps: HashMap<String, HashMap<String, WindowState>>,
}

impl Default for WindowStateFile {
    fn default() -> Self {
        Self {
            version: 1,
            apps: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn json_store_round_trips_named_state() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "myrust-window-state-{}-{unique}.json",
            std::process::id()
        ));
        let store = JsonFileStore::at_path(&path);
        let state = WindowState {
            x: 100.5,
            y: 80.25,
            width: 1200.0,
            height: 800.0,
            maximized: true,
            fullscreen: false,
            display_scale: 1.5,
        };

        store.save("app", "main", &state).expect("save state");
        let loaded = store.load("app", "main").expect("load state");
        assert_eq!(loaded, Some(state));
        assert_eq!(store.load("app", "other").expect("load missing"), None);

        let _ = fs::remove_file(path);
    }
}
