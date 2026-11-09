//! Native text and image clipboard access built on `arboard`.
//!
//! On Linux, an active X11 or Wayland desktop session is required. Headless
//! environments receive an error instead of a process-level panic.

use std::borrow::Cow;

use anyhow::{bail, Context, Result};

/// RGBA image exchanged with the system clipboard.
///
/// Pixels use four bytes per pixel in red, green, blue, alpha order. Data is
/// row-major and starts at the top-left pixel. Each row contains
/// `width * 4` bytes, and `rgba.len()` must equal `width * height * 4`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// RGBA bytes in row-major order, starting at the top-left pixel.
    pub rgba: Vec<u8>,
}

impl ClipboardImage {
    /// Creates an image after validating its RGBA buffer length.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self> {
        let image = Self {
            width,
            height,
            rgba,
        };
        image.validate()?;
        Ok(image)
    }

    fn validate(&self) -> Result<()> {
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .context("clipboard image dimensions overflow usize")?;

        if self.rgba.len() != expected {
            bail!(
                "clipboard image has {} RGBA bytes, expected {expected} for {}x{}",
                self.rgba.len(),
                self.width,
                self.height
            );
        }

        Ok(())
    }
}

/// Stateful system clipboard handle.
///
/// Keep one instance alive while performing clipboard operations. On Linux the
/// clipboard content is hosted by the application that most recently wrote it,
/// so dropping all handles can make the content unavailable to other apps.
pub struct Clipboard {
    inner: arboard::Clipboard,
}

impl Clipboard {
    /// Opens the system clipboard.
    ///
    /// On Linux this requires `DISPLAY` (X11) or `WAYLAND_DISPLAY` (Wayland).
    pub fn new() -> Result<Self> {
        #[cfg(target_os = "linux")]
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            bail!(
                "clipboard unavailable: Linux requires an X11 DISPLAY or Wayland WAYLAND_DISPLAY session"
            );
        }

        let inner = arboard::Clipboard::new().context(
            "failed to open system clipboard; Linux requires an active X11 or Wayland desktop session",
        )?;
        Ok(Self { inner })
    }

    /// Reads UTF-8 text from the clipboard.
    pub fn read_text(&mut self) -> Result<String> {
        self.inner
            .get_text()
            .context("failed to read text from clipboard")
    }

    /// Writes UTF-8 text to the clipboard.
    pub fn write_text(&mut self, text: impl Into<String>) -> Result<()> {
        self.inner
            .set_text(text.into())
            .context("failed to write text to clipboard")
    }

    /// Reads an image and converts it to row-major RGBA bytes.
    pub fn read_image(&mut self) -> Result<ClipboardImage> {
        let image = self
            .inner
            .get_image()
            .context("failed to read image from clipboard")?;
        let width = u32::try_from(image.width).context("clipboard image width exceeds u32")?;
        let height = u32::try_from(image.height).context("clipboard image height exceeds u32")?;
        ClipboardImage::new(width, height, image.bytes.into_owned())
    }

    /// Writes an RGBA image to the clipboard.
    pub fn write_image(&mut self, image: ClipboardImage) -> Result<()> {
        image.validate()?;
        self.inner
            .set_image(arboard::ImageData {
                width: image.width as usize,
                height: image.height as usize,
                bytes: Cow::Owned(image.rgba),
            })
            .context("failed to write image to clipboard")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_rgba_buffer_size() {
        let image = ClipboardImage::new(2, 2, vec![255; 16]).expect("valid image");
        assert_eq!(image.rgba.len(), 16);

        let error = ClipboardImage::new(2, 2, vec![255; 15]).expect_err("invalid image");
        assert!(error.to_string().contains("expected 16"));
    }
}
