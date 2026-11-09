//! Single-instance guarding and activation forwarding.
//!
//! The `single-instance` crate provides process detection. On Unix, this module
//! adds a per-user filesystem socket so a second launch can forward its command
//! line and working directory to the first instance. Windows named-pipe
//! forwarding is not implemented in batch D: Windows falls back to detection
//! only, and activation callbacks are not triggered across processes there.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

type ActivationCallback = Arc<dyn Fn(Activation) + Send + Sync + 'static>;

/// Activation payload forwarded by a second instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activation {
    /// Command-line arguments of the second launch.
    pub args: Vec<String>,
    /// Working directory of the second launch.
    pub working_dir: PathBuf,
}

/// Holds the process-wide single-instance lock until dropped.
pub struct SingleInstance {
    inner: single_instance::SingleInstance,
    name: String,
    #[cfg(unix)]
    listener: Option<Arc<UnixActivationListener>>,
}

impl SingleInstance {
    /// Acquires a named instance lock.
    ///
    /// A second process with the same name receives `is_single() == false`.
    /// On Unix it also forwards its command line to the first process. Windows
    /// performs detection only in batch D.
    pub fn acquire(name: &str) -> Result<Self> {
        let inner = single_instance::SingleInstance::new(name)
            .with_context(|| format!("failed to acquire single-instance lock `{name}`"))?;
        let is_single = inner.is_single();

        #[cfg(unix)]
        let listener = if is_single {
            match UnixActivationListener::bind(name) {
                Ok(listener) => Some(listener),
                Err(error) => {
                    tracing::warn!(%error, "single-instance activation forwarding unavailable");
                    None
                }
            }
        } else {
            if let Err(error) = forward_activation(name, &current_activation()) {
                tracing::warn!(%error, "failed to forward second-instance activation");
            }
            None
        };

        Ok(Self {
            inner,
            name: name.to_string(),
            #[cfg(unix)]
            listener,
        })
    }

    /// Returns whether this process owns the lock.
    pub fn is_single(&self) -> bool {
        self.inner.is_single()
    }

    /// Registers a callback for activation forwarded by later instances.
    ///
    /// Windows has no cross-process forwarding in batch D; the callback is
    /// accepted but will not fire from a second launch there.
    pub fn on_activate<F>(&self, callback: F)
    where
        F: Fn(Activation) + Send + Sync + 'static,
    {
        #[cfg(unix)]
        if let Some(listener) = &self.listener {
            listener.set_callback(Arc::new(callback));
            return;
        }
        let _ = callback;
    }

    /// Returns the lock name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(unix)]
struct UnixActivationListener {
    socket_path: PathBuf,
    callback: Mutex<Option<ActivationCallback>>,
    pending: Mutex<Vec<Activation>>,
}

#[cfg(unix)]
impl UnixActivationListener {
    fn bind(name: &str) -> Result<Arc<Self>> {
        use std::os::unix::net::UnixListener;

        let socket_path = activation_socket_path(name);
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).with_context(|| {
                format!("failed to remove stale socket {}", socket_path.display())
            })?;
        }
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("failed to bind {}", socket_path.display()))?;
        listener
            .set_nonblocking(true)
            .context("failed to set activation socket nonblocking")?;

        let state = Arc::new(Self {
            socket_path,
            callback: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
        });
        let thread_state = state.clone();
        std::thread::Builder::new()
            .name("myrust-single-instance".to_string())
            .spawn(move || thread_state.accept_loop(listener))
            .context("failed to spawn single-instance listener")?;
        Ok(state)
    }

    fn accept_loop(self: Arc<Self>, listener: std::os::unix::net::UnixListener) {
        use std::io::Read;

        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut data = String::new();
                    let _ = stream.read_to_string(&mut data);
                    if let Ok(activation) = serde_json::from_str::<Activation>(&data) {
                        self.deliver(activation);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(_) => break,
            }
        }
    }

    fn deliver(&self, activation: Activation) {
        let callback = self
            .callback
            .lock()
            .expect("activation callback lock poisoned")
            .clone();
        if let Some(callback) = callback {
            callback(activation);
        } else {
            self.pending
                .lock()
                .expect("activation pending lock poisoned")
                .push(activation);
        }
    }

    fn set_callback(&self, callback: ActivationCallback) {
        *self
            .callback
            .lock()
            .expect("activation callback lock poisoned") = Some(callback.clone());
        let pending = std::mem::take(
            &mut *self
                .pending
                .lock()
                .expect("activation pending lock poisoned"),
        );
        for activation in pending {
            callback(activation);
        }
    }
}

#[cfg(unix)]
impl Drop for UnixActivationListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(unix)]
fn forward_activation(name: &str, activation: &Activation) -> Result<()> {
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    let path = activation_socket_path(name);
    let mut last_error = None;
    for _ in 0..20 {
        match UnixStream::connect(&path) {
            Ok(mut stream) => {
                stream.write_all(&serde_json::to_vec(activation)?)?;
                return Ok(());
            }
            Err(error) => {
                last_error = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    Err(anyhow::anyhow!(
        "activation socket unavailable: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| path.display().to_string())
    ))
}

#[cfg(unix)]
fn activation_socket_path(name: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();
    let file = format!("myrust-{hash:016x}.sock");
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    directory.join(file)
}

fn current_activation() -> Activation {
    Activation {
        args: std::env::args().skip(1).collect(),
        working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

/// Returns the platform socket path for a lock name on Unix.
///
/// Returns `None` on non-Unix platforms because named-pipe forwarding is not
/// implemented in batch D.
pub fn activation_socket_path_for(name: &str) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        Some(activation_socket_path(name))
    }
    #[cfg(not(unix))]
    {
        let _ = name;
        None
    }
}

/// Returns whether cross-process activation forwarding is implemented.
pub fn supports_activation_forwarding() -> bool {
    cfg!(unix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_socket_path_is_stable() {
        #[cfg(unix)]
        {
            let first = activation_socket_path_for("test-app").expect("unix socket");
            let second = activation_socket_path_for("test-app").expect("unix socket");
            assert_eq!(first, second);
            assert!(first.to_string_lossy().contains("myrust-"));
        }
        #[cfg(not(unix))]
        assert!(activation_socket_path_for("test-app").is_none());
    }
}
