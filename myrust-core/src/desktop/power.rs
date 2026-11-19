//! Power-management and media-session integration.

#[cfg(target_os = "windows")]
use std::thread::ThreadId;
use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};

/// Kind of sleep that an [`PowerAssertion`] prevents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAssertionKind {
    /// Keep the system running while allowing the display to sleep.
    PreventSleep,
    /// Keep the system and display awake.
    PreventDisplaySleep,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AssertionFlags {
    system: bool,
    display: bool,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Default)]
struct AssertionState {
    system_assertions: usize,
    display_assertions: usize,
}

#[cfg(any(target_os = "windows", test))]
impl AssertionState {
    fn acquire(&mut self, kind: PowerAssertionKind) {
        match kind {
            PowerAssertionKind::PreventSleep => self.system_assertions += 1,
            PowerAssertionKind::PreventDisplaySleep => {
                self.system_assertions += 1;
                self.display_assertions += 1;
            }
        }
    }

    fn release(&mut self, kind: PowerAssertionKind) -> bool {
        match kind {
            PowerAssertionKind::PreventSleep if self.system_assertions > 0 => {
                self.system_assertions -= 1;
                true
            }
            PowerAssertionKind::PreventDisplaySleep
                if self.system_assertions > 0 && self.display_assertions > 0 =>
            {
                self.system_assertions -= 1;
                self.display_assertions -= 1;
                true
            }
            _ => false,
        }
    }

    fn requested_flags(&self) -> AssertionFlags {
        AssertionFlags {
            system: self.system_assertions > 0,
            display: self.display_assertions > 0,
        }
    }
}

/// An RAII sleep inhibitor.
///
/// Dropping the assertion releases the operating-system resource. Windows
/// assertions use thread-local execution state, so acquire and drop them on the
/// same thread. A wrong-thread Windows drop is logged and cannot release the
/// original thread's assertion.
pub struct PowerAssertion {
    kind: PowerAssertionKind,
    reason: String,
    #[cfg(target_os = "windows")]
    owner_thread: ThreadId,
    backend: PowerAssertionBackend,
}

enum PowerAssertionBackend {
    #[cfg(target_os = "macos")]
    MacOs(u32),
    #[cfg(target_os = "windows")]
    Windows,
    #[cfg(target_os = "linux")]
    Linux(std::process::Child),
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Unsupported,
}

impl PowerAssertion {
    /// Acquires a sleep assertion for the lifetime of the returned value.
    pub fn acquire(kind: PowerAssertionKind, reason: impl Into<String>) -> Result<Self> {
        let reason = reason.into();
        validate_assertion_reason(&reason)?;
        let backend = acquire_backend(kind, &reason)?;
        Ok(Self {
            kind,
            reason,
            #[cfg(target_os = "windows")]
            owner_thread: std::thread::current().id(),
            backend,
        })
    }

    /// Prevents idle system sleep while allowing the display to sleep.
    pub fn prevent_sleep(reason: impl Into<String>) -> Result<Self> {
        Self::acquire(PowerAssertionKind::PreventSleep, reason)
    }

    /// Prevents both idle system sleep and display sleep.
    pub fn prevent_display_sleep(reason: impl Into<String>) -> Result<Self> {
        Self::acquire(PowerAssertionKind::PreventDisplaySleep, reason)
    }

    /// Returns the assertion kind.
    pub fn kind(&self) -> PowerAssertionKind {
        self.kind
    }

    /// Returns the human-readable reason passed to the platform backend.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl Drop for PowerAssertion {
    fn drop(&mut self) {
        match &mut self.backend {
            #[cfg(target_os = "macos")]
            PowerAssertionBackend::MacOs(id) => {
                let result = objc2_io_kit::IOPMAssertionRelease(*id);
                if result != objc2_io_kit::kIOReturnSuccess {
                    tracing::warn!(
                        assertion_id = *id,
                        result,
                        "failed to release macOS power assertion"
                    );
                }
            }
            #[cfg(target_os = "windows")]
            PowerAssertionBackend::Windows => {
                if std::thread::current().id() != self.owner_thread {
                    tracing::warn!(
                        id = ?self.owner_thread,
                        "Windows power assertion dropped on a different thread"
                    );
                    return;
                }
                release_windows_assertion(self.kind);
            }
            #[cfg(target_os = "linux")]
            PowerAssertionBackend::Linux(child) => {
                if let Err(error) = child.kill() {
                    tracing::warn!(%error, "failed to stop Linux power inhibitor");
                }
                if let Err(error) = child.wait() {
                    tracing::warn!(%error, "failed to reap Linux power inhibitor");
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
            PowerAssertionBackend::Unsupported => {}
        }
    }
}

/// Process-level keep-running controls.
///
/// `keep_running(true)` holds one process-wide [`PowerAssertion`] and calls with
/// `true` are idempotent. `keep_running(false)` releases it. Prefer
/// [`PowerAssertion`] when the assertion should follow a narrower scope.
#[derive(Debug, Default)]
pub struct Power;

static KEEP_RUNNING: OnceLock<Mutex<Option<PowerAssertion>>> = OnceLock::new();

impl Power {
    /// Enables or disables a process-wide keep-running assertion.
    pub fn keep_running(enabled: bool) -> Result<()> {
        let state = KEEP_RUNNING.get_or_init(|| Mutex::new(None));
        let mut state = state.lock().expect("power assertion lock poisoned");

        if enabled {
            if state.is_none() {
                state.replace(PowerAssertion::prevent_sleep("myrust keep-running")?);
            }
        } else {
            drop(state.take());
        }
        Ok(())
    }

    /// Acquires an assertion that prevents idle system sleep.
    pub fn prevent_sleep(reason: impl Into<String>) -> Result<PowerAssertion> {
        PowerAssertion::prevent_sleep(reason)
    }

    /// Acquires an assertion that prevents idle system and display sleep.
    pub fn prevent_display_sleep(reason: impl Into<String>) -> Result<PowerAssertion> {
        PowerAssertion::prevent_display_sleep(reason)
    }
}

fn validate_assertion_reason(reason: &str) -> Result<()> {
    let reason = reason.trim();
    if reason.is_empty() {
        bail!("power assertion reason must not be empty");
    }
    if reason.len() > 128 {
        bail!("power assertion reason must not exceed 128 bytes");
    }
    Ok(())
}

fn acquire_backend(kind: PowerAssertionKind, reason: &str) -> Result<PowerAssertionBackend> {
    #[cfg(target_os = "linux")]
    {
        acquire_linux_backend(kind, reason)
    }
    #[cfg(target_os = "macos")]
    {
        acquire_macos_backend(kind, reason)
    }
    #[cfg(target_os = "windows")]
    {
        acquire_windows_backend(kind)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (kind, reason);
        bail!("power assertions are not implemented on this platform")
    }
}

#[cfg(target_os = "linux")]
fn acquire_linux_backend(kind: PowerAssertionKind, reason: &str) -> Result<PowerAssertionBackend> {
    use std::{
        io::Read,
        process::{Command, Stdio},
    };

    if kind == PowerAssertionKind::PreventDisplaySleep {
        bail!(
            "Linux display-sleep inhibition is not implemented; use PreventSleep or a desktop-specific screen-saver API"
        );
    }

    let mut child = Command::new("systemd-inhibit")
        .arg("--what=idle:sleep")
        .arg("--who=myrust")
        .arg("--why")
        .arg(reason)
        .arg("--mode=block")
        .arg("sleep")
        .arg("infinity")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to start systemd-inhibit; a systemd login session is required")?;

    std::thread::sleep(Duration::from_millis(50));
    if let Some(status) = child
        .try_wait()
        .context("failed to query systemd-inhibit state")?
    {
        let mut error = String::new();
        if let Some(stderr) = child.stderr.as_mut() {
            let _ = stderr.read_to_string(&mut error);
        }
        bail!(
            "systemd-inhibit exited during startup with {status}: {}",
            error.trim()
        );
    }

    Ok(PowerAssertionBackend::Linux(child))
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn acquire_macos_backend(kind: PowerAssertionKind, reason: &str) -> Result<PowerAssertionBackend> {
    use objc2_core_foundation::CFString;
    use objc2_io_kit::{
        kIOPMAssertionLevelOn, kIOReturnSuccess, IOPMAssertionCreateWithName, IOPMAssertionID,
    };

    let assertion_type = CFString::from_static_str(match kind {
        PowerAssertionKind::PreventSleep => "PreventUserIdleSystemSleep",
        PowerAssertionKind::PreventDisplaySleep => "PreventUserIdleDisplaySleep",
    });
    let assertion_name = CFString::from_str(reason);
    let mut assertion_id: IOPMAssertionID = 0;
    let result = unsafe {
        IOPMAssertionCreateWithName(
            Some(&assertion_type),
            kIOPMAssertionLevelOn,
            Some(&assertion_name),
            &mut assertion_id,
        )
    };
    if result != kIOReturnSuccess {
        bail!("IOPMAssertionCreateWithName failed with IOReturn {result}");
    }
    Ok(PowerAssertionBackend::MacOs(assertion_id))
}

#[cfg(target_os = "windows")]
thread_local! {
    static WINDOWS_ASSERTIONS: std::cell::RefCell<AssertionState> =
        std::cell::RefCell::new(AssertionState::default());
}

#[cfg(target_os = "windows")]
fn acquire_windows_backend(kind: PowerAssertionKind) -> Result<PowerAssertionBackend> {
    let (before, after) = WINDOWS_ASSERTIONS.with(|state| {
        let mut state = state.borrow_mut();
        let before = state.requested_flags();
        state.acquire(kind);
        (before, state.requested_flags())
    });

    if let Err(error) = apply_windows_flags(after) {
        WINDOWS_ASSERTIONS.with(|state| {
            let mut state = state.borrow_mut();
            state.release(kind);
        });
        if apply_windows_flags(before).is_err() {
            tracing::warn!("failed to restore Windows execution state after acquisition error");
        }
        return Err(error);
    }
    Ok(PowerAssertionBackend::Windows)
}

#[cfg(target_os = "windows")]
fn release_windows_assertion(kind: PowerAssertionKind) {
    let (before, after) = WINDOWS_ASSERTIONS.with(|state| {
        let mut state = state.borrow_mut();
        let before = state.requested_flags();
        if !state.release(kind) {
            tracing::warn!("Windows power assertion state underflow");
        }
        (before, state.requested_flags())
    });

    if before != after {
        if let Err(error) = apply_windows_flags(after) {
            tracing::warn!(%error, "failed to update Windows execution state");
        }
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn apply_windows_flags(flags: AssertionFlags) -> Result<()> {
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
        EXECUTION_STATE,
    };

    let mut requested = ES_CONTINUOUS;
    if flags.system {
        requested |= ES_SYSTEM_REQUIRED;
    }
    if flags.display {
        requested |= ES_DISPLAY_REQUIRED;
    }

    let previous = unsafe { SetThreadExecutionState(requested) };
    if previous == EXECUTION_STATE(0) {
        bail!("SetThreadExecutionState failed");
    }
    Ok(())
}

/// Now-playing metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NowPlayingMetadata {
    /// Track title.
    pub title: Option<String>,
    /// Track artist.
    pub artist: Option<String>,
    /// Album name.
    pub album: Option<String>,
    /// Total track duration.
    pub duration: Option<Duration>,
    /// Current playback position.
    pub progress: Option<Duration>,
    /// Cover-art URL or platform-specific URI.
    pub cover_url: Option<String>,
}

/// Media-control event forwarded from the operating system.
#[derive(Debug, Clone, PartialEq)]
pub enum NowPlayingEvent {
    /// Play requested.
    Play,
    /// Pause requested.
    Pause,
    /// Play/pause toggle requested.
    Toggle,
    /// Next track requested.
    Next,
    /// Previous track requested.
    Previous,
    /// Stop requested.
    Stop,
    /// Absolute seek position requested.
    SeekTo(Duration),
    /// Relative seek requested; positive is forward.
    SeekBy(Duration),
    /// Other platform media-control event not normalized by this wrapper.
    Other,
}

/// OS media controls and now-playing metadata.
///
/// Uses the system MPRIS (Linux), Now Playing (macOS), or System Media
/// Transport Controls (Windows) integration. Linux requires a D-Bus session
/// bus and an MPRIS-compatible desktop; creation or attachment fails with an
/// explicit error when the service cannot be started.
pub struct NowPlaying {
    controls: MediaControls,
}

impl NowPlaying {
    /// Creates media controls for an application.
    ///
    /// Linux requires a D-Bus session bus and an MPRIS-compatible desktop.
    pub fn new(app_name: impl Into<String>, dbus_name: impl Into<String>) -> Result<Self> {
        let display_name = app_name.into();
        let dbus_name = dbus_name.into();
        let config = PlatformConfig {
            display_name: &display_name,
            dbus_name: &dbus_name,
            hwnd: None,
        };
        let controls = MediaControls::new(config).context("failed to create OS media controls")?;
        Ok(Self { controls })
    }

    /// Attaches a media-key event callback.
    pub fn on_event<F>(&mut self, callback: F) -> Result<()>
    where
        F: Fn(NowPlayingEvent) + Send + 'static,
    {
        self.controls
            .attach(move |event| callback(convert_event(event)))
            .context("failed to attach media controls")
    }

    /// Updates now-playing metadata and playback state.
    pub fn set_metadata(&mut self, metadata: &NowPlayingMetadata, playing: bool) -> Result<()> {
        let media_metadata = MediaMetadata {
            title: metadata.title.as_deref(),
            album: metadata.album.as_deref(),
            artist: metadata.artist.as_deref(),
            cover_url: metadata.cover_url.as_deref(),
            duration: metadata.duration,
        };
        self.controls
            .set_metadata(media_metadata)
            .context("failed to set now-playing metadata")?;
        let playback = if playing {
            MediaPlayback::Playing {
                progress: metadata.progress.map(MediaPosition),
            }
        } else {
            MediaPlayback::Paused {
                progress: metadata.progress.map(MediaPosition),
            }
        };
        self.controls
            .set_playback(playback)
            .context("failed to set media playback state")
    }
}

fn convert_event(event: MediaControlEvent) -> NowPlayingEvent {
    match event {
        MediaControlEvent::Play => NowPlayingEvent::Play,
        MediaControlEvent::Pause => NowPlayingEvent::Pause,
        MediaControlEvent::Toggle => NowPlayingEvent::Toggle,
        MediaControlEvent::Next => NowPlayingEvent::Next,
        MediaControlEvent::Previous => NowPlayingEvent::Previous,
        MediaControlEvent::Stop => NowPlayingEvent::Stop,
        MediaControlEvent::SetPosition(position) => NowPlayingEvent::SeekTo(position.0),
        MediaControlEvent::Seek(direction) | MediaControlEvent::SeekBy(direction, _) => {
            let duration = match event {
                MediaControlEvent::SeekBy(_, duration) => duration,
                _ => Duration::from_secs(10),
            };
            match direction {
                souvlaki::SeekDirection::Forward => NowPlayingEvent::SeekBy(duration),
                souvlaki::SeekDirection::Backward => NowPlayingEvent::SeekBy(duration),
            }
        }
        _ => NowPlayingEvent::Other,
    }
}

/// Cross-platform login-item configuration.
///
/// macOS requires a packaged `.app` with a stable bundle identity for reliable
/// login-item registration. A raw development binary is not a distributable
/// login item. Windows writes a user registry entry; Linux writes an XDG
/// autostart entry.
pub struct LoginItem {
    inner: auto_launch::AutoLaunch,
    enabled: std::sync::Arc<Mutex<bool>>,
}

impl LoginItem {
    /// Creates a login item for the current executable.
    ///
    /// macOS login-item registration is only reliable for a packaged `.app`
    /// with a bundle identity; a raw development binary is not a distributable
    /// login item. Windows and Linux write user-level startup registrations.
    pub fn new(app_name: impl Into<String>) -> Result<Self> {
        let app_name = app_name.into();
        let app_path = std::env::current_exe()
            .context("failed to resolve current executable")?
            .display()
            .to_string();
        let inner = auto_launch::AutoLaunchBuilder::new()
            .set_app_name(&app_name)
            .set_app_path(&app_path)
            .build()
            .context("failed to create login-item configuration")?;
        Ok(Self {
            inner,
            enabled: std::sync::Arc::new(Mutex::new(false)),
        })
    }

    /// Enables or disables launch at login.
    pub fn set(&self, enabled: bool) -> Result<()> {
        if enabled {
            self.inner.enable().context("failed to enable login item")?;
        } else {
            self.inner
                .disable()
                .context("failed to disable login item")?;
        }
        *self.enabled.lock().expect("login-item lock poisoned") = enabled;
        Ok(())
    }

    /// Returns whether the login item is registered.
    pub fn is_enabled(&self) -> Result<bool> {
        self.inner
            .is_enabled()
            .context("failed to query login-item state")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assertion_state_tracks_system_and_display_counts() {
        let mut state = AssertionState::default();
        assert_eq!(state.requested_flags(), AssertionFlags::default());

        state.acquire(PowerAssertionKind::PreventSleep);
        assert_eq!(
            state.requested_flags(),
            AssertionFlags {
                system: true,
                display: false,
            }
        );

        state.acquire(PowerAssertionKind::PreventSleep);
        state.acquire(PowerAssertionKind::PreventDisplaySleep);
        assert_eq!(
            state.requested_flags(),
            AssertionFlags {
                system: true,
                display: true,
            }
        );

        assert!(state.release(PowerAssertionKind::PreventDisplaySleep));
        assert_eq!(
            state.requested_flags(),
            AssertionFlags {
                system: true,
                display: false,
            }
        );
        assert!(state.release(PowerAssertionKind::PreventSleep));
        assert!(state.release(PowerAssertionKind::PreventSleep));
        assert_eq!(state.requested_flags(), AssertionFlags::default());
        assert!(!state.release(PowerAssertionKind::PreventSleep));
    }

    #[test]
    fn assertion_reason_is_required_and_bounded() {
        assert!(validate_assertion_reason("").is_err());
        assert!(validate_assertion_reason("  ").is_err());
        assert!(validate_assertion_reason("exporting a video").is_ok());
        assert!(validate_assertion_reason(&"x".repeat(129)).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_display_assertion_is_explicitly_unsupported() {
        let error = match PowerAssertion::prevent_display_sleep("presenting") {
            Ok(_) => panic!("Linux display inhibition must not silently succeed"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("not implemented"));
    }
}
