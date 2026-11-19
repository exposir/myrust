//! Polling clipboard watcher with change debouncing.
//!
//! The watcher reads text first and falls back to RGBA images. Clipboard
//! platforms do not all expose native change notifications, so polling is
//! explicit rather than pretending to be event-driven.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};

use super::clipboard::ClipboardImage;

/// Clipboard content surfaced by [`ClipboardWatcher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardContent {
    /// The clipboard contains no supported text or image data.
    Empty,
    /// UTF-8 text.
    Text(String),
    /// Top-left-origin RGBA image data.
    Image(ClipboardImage),
}

/// Polling and debounce configuration for [`ClipboardWatcher`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipboardWatchOptions {
    /// Time between clipboard samples.
    pub interval: Duration,
    /// Time a changed value must remain stable before it is delivered.
    pub debounce: Duration,
    /// Whether image data is sampled when text is unavailable.
    pub watch_images: bool,
}

impl Default for ClipboardWatchOptions {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(250),
            debounce: Duration::from_millis(250),
            watch_images: true,
        }
    }
}

impl ClipboardWatchOptions {
    fn validate(self) -> Result<Self> {
        if self.interval.is_zero() {
            bail!("clipboard watch interval must be greater than zero");
        }
        Ok(self)
    }
}

/// Background clipboard watcher.
#[derive(Debug, Clone)]
pub struct ClipboardWatcher {
    options: ClipboardWatchOptions,
}

impl ClipboardWatcher {
    /// Creates a watcher after validating the polling configuration.
    pub fn new(options: ClipboardWatchOptions) -> Result<Self> {
        Ok(Self {
            options: options.validate()?,
        })
    }

    /// Starts polling on a background thread.
    ///
    /// Opening the platform clipboard happens before the thread starts, so a
    /// headless or unavailable clipboard returns an explicit error here. The
    /// callback runs on the polling thread and is not called for the initial
    /// empty clipboard.
    pub fn watch<F>(&self, mut callback: F) -> Result<ClipboardWatchHandle>
    where
        F: FnMut(ClipboardContent) + Send + 'static,
    {
        let mut clipboard = open_arboard_clipboard()?;
        let control = Arc::new(WatchControl::new());
        let thread_control = control.clone();
        let options = self.options;
        let thread = std::thread::Builder::new()
            .name("myrust-clipboard-watch".to_string())
            .spawn(move || {
                let started = Instant::now();
                let mut state = ClipboardWatchState::new(options.debounce);
                loop {
                    if thread_control.is_stopped() {
                        break;
                    }

                    match read_clipboard(&mut clipboard, options.watch_images) {
                        Ok(content) => {
                            if let Some(content) = state.observe(started.elapsed(), content) {
                                callback(content);
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%error, "clipboard polling failed");
                        }
                    }

                    thread_control.wait(options.interval);
                }
            })
            .context("failed to spawn clipboard watcher")?;

        Ok(ClipboardWatchHandle {
            control,
            thread: Some(thread),
        })
    }

    /// Returns the validated polling configuration.
    pub fn options(&self) -> ClipboardWatchOptions {
        self.options
    }
}

/// Stops the background clipboard watcher when dropped.
pub struct ClipboardWatchHandle {
    control: Arc<WatchControl>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ClipboardWatchHandle {
    /// Requests shutdown without waiting for the polling thread.
    pub fn stop(&self) {
        self.control.stop();
    }

    /// Returns whether shutdown has been requested.
    pub fn is_stopped(&self) -> bool {
        self.control.is_stopped()
    }
}

impl Drop for ClipboardWatchHandle {
    fn drop(&mut self) {
        self.control.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct WatchControl {
    stopped: AtomicBool,
    wait_lock: Mutex<()>,
    wait_condvar: Condvar,
}

impl WatchControl {
    fn new() -> Self {
        Self {
            stopped: AtomicBool::new(false),
            wait_lock: Mutex::new(()),
            wait_condvar: Condvar::new(),
        }
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.wait_condvar.notify_all();
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    fn wait(&self, interval: Duration) {
        let guard = self
            .wait_lock
            .lock()
            .expect("clipboard watch lock poisoned");
        let _ = self
            .wait_condvar
            .wait_timeout_while(guard, interval, |_| !self.is_stopped())
            .expect("clipboard watch lock poisoned");
    }
}

#[derive(Debug)]
struct ClipboardWatchState {
    debounce: Duration,
    pending: Option<(ClipboardContent, Duration)>,
    emitted: ClipboardContent,
}

impl ClipboardWatchState {
    fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            pending: None,
            emitted: ClipboardContent::Empty,
        }
    }

    fn observe(&mut self, now: Duration, content: ClipboardContent) -> Option<ClipboardContent> {
        if content == self.emitted {
            self.pending = None;
            return None;
        }
        if self.debounce.is_zero() {
            self.pending = None;
            self.emitted = content.clone();
            return Some(content);
        }

        match &self.pending {
            Some((pending, observed_at)) if pending == &content => {
                if now.saturating_sub(*observed_at) >= self.debounce {
                    self.pending = None;
                    self.emitted = content.clone();
                    Some(content)
                } else {
                    None
                }
            }
            _ => {
                self.pending = Some((content, now));
                None
            }
        }
    }
}

fn open_arboard_clipboard() -> Result<arboard::Clipboard> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        bail!("clipboard watcher unavailable: Linux requires a DISPLAY or WAYLAND_DISPLAY session");
    }

    arboard::Clipboard::new().context("failed to open the clipboard for polling")
}

fn read_clipboard(
    clipboard: &mut arboard::Clipboard,
    watch_images: bool,
) -> Result<ClipboardContent> {
    match clipboard.get_text() {
        Ok(text) => return Ok(ClipboardContent::Text(text)),
        Err(arboard::Error::ContentNotAvailable) => {}
        Err(error) => return Err(error).context("failed to read clipboard text"),
    }

    if !watch_images {
        return Ok(ClipboardContent::Empty);
    }

    match clipboard.get_image() {
        Ok(image) => {
            let width = u32::try_from(image.width).context("clipboard image width exceeds u32")?;
            let height =
                u32::try_from(image.height).context("clipboard image height exceeds u32")?;
            Ok(ClipboardContent::Image(ClipboardImage::new(
                width,
                height,
                image.bytes.into_owned(),
            )?))
        }
        Err(arboard::Error::ContentNotAvailable) => Ok(ClipboardContent::Empty),
        Err(error) => Err(error).context("failed to read clipboard image"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn state_emits_only_the_stable_debounced_value() {
        let mut state = ClipboardWatchState::new(ms(200));

        assert_eq!(
            state.observe(ms(0), ClipboardContent::Text("a".into())),
            None
        );
        assert_eq!(
            state.observe(ms(100), ClipboardContent::Text("ab".into())),
            None
        );
        assert_eq!(
            state.observe(ms(250), ClipboardContent::Text("ab".into())),
            None
        );
        assert_eq!(
            state.observe(ms(300), ClipboardContent::Text("ab".into())),
            Some(ClipboardContent::Text("ab".into()))
        );
    }

    #[test]
    fn state_suppresses_duplicates_and_emits_clear() {
        let mut state = ClipboardWatchState::new(Duration::ZERO);
        assert_eq!(
            state.observe(ms(0), ClipboardContent::Text("hello".into())),
            Some(ClipboardContent::Text("hello".into()))
        );
        assert_eq!(
            state.observe(ms(10), ClipboardContent::Text("hello".into())),
            None
        );
        assert_eq!(
            state.observe(ms(20), ClipboardContent::Empty),
            Some(ClipboardContent::Empty)
        );
    }

    #[test]
    fn initial_empty_clipboard_is_not_a_change() {
        let mut state = ClipboardWatchState::new(Duration::ZERO);
        assert_eq!(state.observe(ms(0), ClipboardContent::Empty), None);
        assert_eq!(state.observe(ms(1), ClipboardContent::Empty), None);
    }

    #[test]
    fn watcher_rejects_zero_interval() {
        let error = ClipboardWatcher::new(ClipboardWatchOptions {
            interval: Duration::ZERO,
            ..ClipboardWatchOptions::default()
        })
        .expect_err("zero interval must fail");
        assert!(error.to_string().contains("greater than zero"));
    }
}
