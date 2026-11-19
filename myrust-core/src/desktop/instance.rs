//! Single-instance guarding and activation forwarding.
//!
//! The `single-instance` crate provides process detection. On Unix, this module
//! adds a per-user filesystem socket so a second launch can forward its command
//! line and working directory to the first instance. On Windows it uses a
//! namespaced local socket backed by a named pipe. Other platforms retain
//! detection-only behavior with an explicit capability query.

use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[cfg(windows)]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Weak,
    },
    time::Duration,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

type ActivationCallback = Arc<dyn Fn(Activation) + Send + Sync + 'static>;

const MAX_ACTIVATION_BYTES: usize = 1024 * 1024;
const ACTIVATION_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

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
    #[cfg(any(unix, windows))]
    listener: Option<Arc<dyn ActivationListener>>,
}

impl SingleInstance {
    /// Acquires a named instance lock.
    ///
    /// A second process with the same name receives `is_single() == false`.
    /// Unix and Windows also forward its command line to the first process.
    /// Other platforms perform detection only.
    pub fn acquire(name: &str) -> Result<Self> {
        let inner = single_instance::SingleInstance::new(name)
            .with_context(|| format!("failed to acquire single-instance lock `{name}`"))?;
        let is_single = inner.is_single();

        #[cfg(any(unix, windows))]
        let listener: Option<Arc<dyn ActivationListener>> = if is_single {
            match PlatformActivationListener::bind(name) {
                Ok(listener) => Some(listener as Arc<dyn ActivationListener>),
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
            #[cfg(any(unix, windows))]
            listener,
        })
    }

    /// Returns whether this process owns the lock.
    pub fn is_single(&self) -> bool {
        self.inner.is_single()
    }

    /// Registers a callback for activation forwarded by later instances.
    ///
    /// Activations that arrive before the callback is registered are queued.
    /// Platforms without cross-process forwarding accept the callback but
    /// never invoke it from a second launch.
    pub fn on_activate<F>(&self, callback: F)
    where
        F: Fn(Activation) + Send + Sync + 'static,
    {
        #[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
struct ActivationDispatcher {
    callback: Mutex<Option<ActivationCallback>>,
    pending: Mutex<Vec<Activation>>,
}

#[cfg(any(unix, windows))]
impl ActivationDispatcher {
    fn new() -> Self {
        Self {
            callback: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
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

#[cfg(any(unix, windows))]
trait ActivationListener: Send + Sync {
    fn set_callback(&self, callback: ActivationCallback);
}

#[cfg(unix)]
type PlatformActivationListener = UnixActivationListener;
#[cfg(windows)]
type PlatformActivationListener = WindowsActivationListener;

#[cfg(unix)]
struct UnixActivationListener {
    socket_path: PathBuf,
    dispatcher: Arc<ActivationDispatcher>,
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
            dispatcher: Arc::new(ActivationDispatcher::new()),
        });
        let thread_state = state.clone();
        std::thread::Builder::new()
            .name("myrust-single-instance".to_string())
            .spawn(move || thread_state.accept_loop(listener))
            .context("failed to spawn single-instance listener")?;
        Ok(state)
    }

    fn accept_loop(self: Arc<Self>, listener: std::os::unix::net::UnixListener) {
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(ACTIVATION_READ_TIMEOUT));
                    match read_activation(&mut stream) {
                        Ok(activation) => self.dispatcher.deliver(activation),
                        Err(error) => {
                            tracing::warn!(%error, "invalid single-instance activation frame");
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(error) => {
                    tracing::debug!(%error, "single-instance activation listener stopped");
                    break;
                }
            }
        }
    }
}

#[cfg(unix)]
impl ActivationListener for UnixActivationListener {
    fn set_callback(&self, callback: ActivationCallback) {
        self.dispatcher.set_callback(callback);
    }
}

#[cfg(unix)]
impl Drop for UnixActivationListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(windows)]
struct WindowsActivationListener {
    pipe_name: String,
    dispatcher: Arc<ActivationDispatcher>,
    stopped: AtomicBool,
}

#[cfg(windows)]
impl WindowsActivationListener {
    fn bind(name: &str) -> Result<Arc<Self>> {
        use interprocess::local_socket::{
            prelude::*, GenericNamespaced, ListenerNonblockingMode, ListenerOptions,
        };

        let pipe_name = activation_endpoint_name(name);
        let socket_name = pipe_name
            .as_str()
            .to_ns_name::<GenericNamespaced>()
            .context("failed to convert activation name to a Windows pipe name")?;
        let listener = ListenerOptions::new()
            .name(socket_name)
            .create_sync()
            .with_context(|| format!("failed to create activation named pipe `{pipe_name}`"))?;
        listener
            .set_nonblocking(ListenerNonblockingMode::Accept)
            .context("failed to set activation named pipe nonblocking")?;

        let state = Arc::new(Self {
            pipe_name,
            dispatcher: Arc::new(ActivationDispatcher::new()),
            stopped: AtomicBool::new(false),
        });
        let thread_state = Arc::downgrade(&state);
        std::thread::Builder::new()
            .name("myrust-single-instance".to_string())
            .spawn(move || accept_loop(thread_state, listener))
            .context("failed to spawn single-instance named-pipe listener")?;
        Ok(state)
    }
}

#[cfg(windows)]
fn accept_loop(
    state: Weak<WindowsActivationListener>,
    listener: interprocess::local_socket::Listener,
) {
    use interprocess::local_socket::prelude::*;

    loop {
        let Some(state_ref) = state.upgrade() else {
            break;
        };
        if state_ref.stopped.load(Ordering::Acquire) {
            break;
        }

        match listener.accept() {
            Ok(mut stream) => {
                let _ = stream.set_recv_timeout(Some(ACTIVATION_READ_TIMEOUT));
                match read_activation(&mut stream) {
                    Ok(activation) => state_ref.dispatcher.deliver(activation),
                    Err(error) => {
                        tracing::warn!(%error, "invalid single-instance activation frame");
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                drop(state_ref);
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                tracing::debug!(
                    %error,
                    pipe = %state_ref.pipe_name,
                    "single-instance named-pipe listener stopped"
                );
                break;
            }
        }
    }
}

#[cfg(windows)]
impl ActivationListener for WindowsActivationListener {
    fn set_callback(&self, callback: ActivationCallback) {
        self.dispatcher.set_callback(callback);
    }
}

#[cfg(windows)]
impl Drop for WindowsActivationListener {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
}

#[cfg(unix)]
fn forward_activation(name: &str, activation: &Activation) -> Result<()> {
    use std::os::unix::net::UnixStream;

    let path = activation_socket_path(name);
    let mut last_error = None;
    for _ in 0..20 {
        match UnixStream::connect(&path) {
            Ok(mut stream) => {
                write_activation(&mut stream, activation)?;
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

#[cfg(windows)]
fn forward_activation(name: &str, activation: &Activation) -> Result<()> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, Stream};

    let pipe_name = activation_endpoint_name(name);
    let mut last_error = None;
    for _ in 0..20 {
        let socket_name = pipe_name
            .as_str()
            .to_ns_name::<GenericNamespaced>()
            .context("failed to convert activation name to a Windows pipe name")?;
        match Stream::connect(socket_name) {
            Ok(mut stream) => {
                write_activation(&mut stream, activation)?;
                return Ok(());
            }
            Err(error) => {
                last_error = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    Err(anyhow::anyhow!(
        "activation named pipe unavailable: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| pipe_name.clone())
    ))
}

#[cfg(unix)]
fn activation_socket_path(name: &str) -> PathBuf {
    let file = format!("{}.sock", activation_endpoint_name(name));
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    directory.join(file)
}

fn activation_endpoint_name(name: &str) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    format!("myrust-{:016x}", hasher.finish())
}

fn write_activation(writer: &mut impl Write, activation: &Activation) -> Result<()> {
    let frame = activation_frame(activation)?;
    writer
        .write_all(&frame)
        .context("failed to write activation frame")
}

fn read_activation(reader: &mut impl Read) -> Result<Activation> {
    let mut header = [0u8; 4];
    reader
        .read_exact(&mut header)
        .context("failed to read activation frame length")?;
    let payload_len = u32::from_be_bytes(header) as usize;
    if payload_len > MAX_ACTIVATION_BYTES {
        bail!("activation payload exceeds {MAX_ACTIVATION_BYTES} bytes");
    }
    let mut payload = vec![0u8; payload_len];
    reader
        .read_exact(&mut payload)
        .context("failed to read activation payload")?;
    serde_json::from_slice(&payload).context("failed to parse activation payload")
}

fn activation_frame(activation: &Activation) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(activation).context("failed to encode activation payload")?;
    if payload.len() > MAX_ACTIVATION_BYTES {
        bail!("activation payload exceeds {MAX_ACTIVATION_BYTES} bytes");
    }
    let payload_len =
        u32::try_from(payload.len()).context("activation payload length exceeds u32")?;

    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&payload_len.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

#[cfg(test)]
fn decode_activation_frame(frame: &[u8]) -> Result<Activation> {
    let (header, payload) = frame
        .split_at_checked(4)
        .context("activation frame is shorter than its 4-byte length header")?;
    let payload_len = u32::from_be_bytes(
        header
            .try_into()
            .expect("the split header is exactly four bytes"),
    ) as usize;
    if payload_len > MAX_ACTIVATION_BYTES {
        bail!("activation payload exceeds {MAX_ACTIVATION_BYTES} bytes");
    }
    if payload.len() != payload_len {
        bail!(
            "activation frame declared {payload_len} payload bytes but contained {}",
            payload.len()
        );
    }
    serde_json::from_slice(payload).context("failed to parse activation payload")
}

fn current_activation() -> Activation {
    Activation {
        args: std::env::args().skip(1).collect(),
        working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

/// Returns the platform endpoint path for a lock name.
///
/// Unix returns the filesystem socket path. Windows returns the named-pipe
/// path (`\\.\pipe\...`). Other platforms return `None` because cross-process
/// activation forwarding is not implemented there.
pub fn activation_socket_path_for(name: &str) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        Some(activation_socket_path(name))
    }
    #[cfg(windows)]
    {
        Some(PathBuf::from(format!(
            r"\\.\pipe\{}",
            activation_endpoint_name(name)
        )))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = name;
        None
    }
}

/// Returns whether cross-process activation forwarding is implemented.
pub fn supports_activation_forwarding() -> bool {
    cfg!(any(unix, windows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn sample_activation() -> Activation {
        Activation {
            args: vec![
                "--open".to_string(),
                "/tmp/caf\u{00e9}.txt".to_string(),
                "--flag=\u{503c}".to_string(),
            ],
            working_dir: PathBuf::from("/tmp/work tree"),
        }
    }

    #[test]
    fn activation_frame_round_trips_utf8_and_spaces() {
        let activation = sample_activation();
        let frame = activation_frame(&activation).expect("encode frame");
        let decoded = decode_activation_frame(&frame).expect("decode frame");
        assert_eq!(decoded, activation);

        let mut reader = Cursor::new(frame);
        let streamed = read_activation(&mut reader).expect("read framed activation");
        assert_eq!(streamed, activation);
    }

    #[test]
    fn activation_frame_rejects_truncated_malformed_and_oversized_data() {
        assert!(decode_activation_frame(&[0, 0]).is_err());
        assert!(decode_activation_frame(&[0, 0, 0, 2, b'{']).is_err());

        let mut frame = activation_frame(&sample_activation()).expect("encode frame");
        frame.push(0);
        assert!(decode_activation_frame(&frame).is_err());

        let oversized = (MAX_ACTIVATION_BYTES as u32 + 1).to_be_bytes();
        assert!(decode_activation_frame(&oversized).is_err());
    }

    #[test]
    fn activation_endpoint_is_stable_and_namespaced() {
        let first = activation_endpoint_name("test-app");
        let second = activation_endpoint_name("test-app");
        assert_eq!(first, second);
        assert!(first.starts_with("myrust-"));
    }

    #[test]
    fn activation_socket_path_is_stable() {
        let first = activation_socket_path_for("test-app");
        let second = activation_socket_path_for("test-app");
        assert_eq!(first, second);

        #[cfg(any(unix, windows))]
        {
            let path = first.expect("platform activation endpoint");
            assert!(path.to_string_lossy().contains("myrust-"));
        }
        #[cfg(not(any(unix, windows)))]
        assert!(first.is_none());
    }

    #[test]
    fn activation_forwarding_support_is_explicit() {
        assert_eq!(supports_activation_forwarding(), cfg!(any(unix, windows)));
    }
}
