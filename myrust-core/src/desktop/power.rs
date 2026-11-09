//! Power-management and media-session integration.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};

/// Keeps the system awake while enabled.
///
/// No cross-platform inhibitor backend is included in batch C. On unsupported
/// targets this returns an explicit error; it does not pretend to hold a power
/// assertion.
#[derive(Debug, Default)]
pub struct Power;

impl Power {
    /// Enables or disables a keep-running assertion.
    pub fn keep_running(enabled: bool) -> Result<()> {
        if !enabled {
            return Ok(());
        }
        bail!("Power::keep_running is not implemented on this platform in Phase 2 batch C")
    }
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
    enabled: Arc<Mutex<bool>>,
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
            enabled: Arc::new(Mutex::new(false)),
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
