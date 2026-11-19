//! Host-driven video frame playback primitives and widget.
//!
//! MyRust does not decode media. A host pushes timestamped RGBA frames as
//! [`ImageHandle`] values; [`VideoPlayer`] chooses a frame deterministically
//! from its playback position, and the [`video`] widget emits the same image
//! command as a still picture.

use std::{sync::Arc, time::Duration};

use super::{
    context::{Context, ImageFit, ImageStyle},
    image::{ImageHandle, ImagePixels},
    paint::Color,
};

/// One decoded frame at a presentation timestamp.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Presentation timestamp from the start of the stream.
    pub pts: Duration,
    /// Decoded frame pixels.
    pub image: ImageHandle,
}

/// State returned by [`video`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VideoState {
    /// Player was playing when the frame was built.
    pub playing: bool,
    /// Playback reached the end and is not looping.
    pub ended: bool,
    /// Current playback position.
    pub position: Duration,
    /// Selected frame index, if any.
    pub frame_index: Option<usize>,
    /// The widget drew a placeholder rather than a frame.
    pub placeholder: bool,
}

/// Visual options for [`video`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoStyle {
    /// How the frame fills the box.
    pub fit: ImageFit,
    /// Corner radius.
    pub radius: f32,
    /// Draw the frame in grayscale.
    pub grayscale: bool,
    /// Placeholder/background color.
    pub background: Color,
}

impl Default for VideoStyle {
    fn default() -> Self {
        Self {
            fit: ImageFit::Contain,
            radius: 0.0,
            grayscale: false,
            background: Color::rgb(16, 16, 16),
        }
    }
}

/// Host-independent video playback state.
#[derive(Debug, Clone)]
pub struct VideoPlayer {
    frames: Vec<VideoFrame>,
    position: Duration,
    duration: Option<Duration>,
    playing: bool,
    loop_playback: bool,
    ended: bool,
}

impl VideoPlayer {
    /// Creates a paused player.
    pub fn new(loop_playback: bool) -> Self {
        Self {
            frames: Vec::new(),
            position: Duration::ZERO,
            duration: None,
            playing: false,
            loop_playback,
            ended: false,
        }
    }

    /// Adds or replaces a frame at its presentation timestamp.
    pub fn push_frame(&mut self, pts: Duration, image: ImageHandle) {
        let frame = VideoFrame { pts, image };
        match self.frames.binary_search_by_key(&pts, |frame| frame.pts) {
            Ok(index) => self.frames[index] = frame,
            Err(index) => self.frames.insert(index, frame),
        }
        if self.duration.is_none() {
            self.refresh_duration();
        }
    }

    /// Adds decoded pixels as a frame. The host owns decoding.
    pub fn push_pixels(
        &mut self,
        pts: Duration,
        id: impl Into<String>,
        pixels: ImagePixels,
    ) -> Result<(), super::image::ImageDataError> {
        if pixels.width() == 0 || pixels.height() == 0 {
            return Err(super::image::ImageDataError::EmptyDimensions);
        }
        self.push_frame(pts, ImageHandle::from_pixels(id, pixels));
        Ok(())
    }

    /// Sets the stream duration. `None` derives it from the last frame.
    pub fn set_duration(&mut self, duration: Option<Duration>) {
        self.duration = duration;
        if self.duration.is_none() {
            self.refresh_duration();
        }
    }

    /// Returns the stream duration, if any frame or explicit value exists.
    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// Starts playback from the current position.
    pub fn play(&mut self) {
        if !self.frames.is_empty() && !self.ended {
            self.playing = true;
        }
    }

    /// Pauses playback.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Returns whether playback is advancing.
    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    /// Returns whether playback is at the stream end.
    pub const fn is_ended(&self) -> bool {
        self.ended
    }

    /// Returns the current playback position.
    pub const fn position(&self) -> Duration {
        self.position
    }

    /// Seeks to `position`, clamped to the stream duration.
    pub fn seek(&mut self, position: Duration) {
        let maximum = self.duration.unwrap_or(Duration::ZERO);
        self.position = position.min(maximum);
        self.ended = self
            .duration
            .is_some_and(|duration| duration > Duration::ZERO && self.position >= duration);
    }

    /// Advances playback by a deterministic delta.
    ///
    /// Returns whether the selected frame or end state changed.
    pub fn advance(&mut self, dt: Duration) -> bool {
        if !self.playing || dt.is_zero() {
            return false;
        }
        let before = (self.frame_index(), self.ended);
        let Some(duration) = self.duration else {
            self.playing = false;
            self.ended = !self.frames.is_empty();
            return before != (self.frame_index(), self.ended);
        };
        if duration.is_zero() {
            self.playing = false;
            self.ended = !self.frames.is_empty();
            return before != (self.frame_index(), self.ended);
        }
        self.position = self.position.saturating_add(dt);
        if self.position >= duration {
            if self.loop_playback {
                let nanos = self.position.as_nanos() % duration.as_nanos().max(1);
                self.position = Duration::from_nanos(nanos as u64);
                self.ended = false;
            } else {
                self.position = duration;
                self.playing = false;
                self.ended = true;
            }
        }
        before != (self.frame_index(), self.ended)
    }

    /// Number of frames currently available.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Returns the frame selected at the current position.
    pub fn current_frame(&self) -> Option<&VideoFrame> {
        self.frame_index().and_then(|index| self.frames.get(index))
    }

    /// Returns the selected frame index.
    pub fn frame_index(&self) -> Option<usize> {
        if self.frames.is_empty() {
            return None;
        }
        let mut selected = 0;
        for (index, frame) in self.frames.iter().enumerate() {
            if frame.pts <= self.position {
                selected = index;
            } else {
                break;
            }
        }
        Some(selected)
    }

    fn refresh_duration(&mut self) {
        self.duration = self.frames.last().map(|frame| frame.pts);
    }
}

/// Draws a player's current frame as an image command.
///
/// No frame yields a one-pixel background placeholder. The widget never
/// panics for empty or malformed media; the host owns decoding and audio
/// synchronization.
pub fn video(
    context: &mut Context,
    id: &str,
    player: &VideoPlayer,
    width: f32,
    height: f32,
    style: VideoStyle,
) -> VideoState {
    let source = player.current_frame().map(|frame| frame.image.clone());
    let placeholder = source.is_none();
    let source = source.unwrap_or_else(|| placeholder_handle(style.background));
    let image_style = ImageStyle {
        fit: style.fit,
        radius: style.radius,
        grayscale: style.grayscale,
    };
    let _ = context.image_handle(id, source, width, height, image_style);
    VideoState {
        playing: player.is_playing(),
        ended: player.is_ended(),
        position: player.position(),
        frame_index: player.frame_index(),
        placeholder,
    }
}

fn placeholder_handle(background: Color) -> ImageHandle {
    let pixels = Arc::from([
        background.r,
        background.g,
        background.b,
        255,
    ]);
    let data = ImagePixels::new(pixels, 1, 1).expect("one pixel is valid");
    ImageHandle::from_pixels("video-placeholder", data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        image::ImagePixels,
        layout::Size,
        paint::DrawCommand,
    };

    fn frame(_pts: u64, shade: u8) -> ImageHandle {
        let pixels = Arc::from([shade, shade, shade, 255]);
        ImageHandle::from_pixels(
            "video",
            ImagePixels::new(pixels, 1, 1).expect("valid frame"),
        )
    }

    #[test]
    fn selects_frames_at_boundaries_and_wraps() {
        let mut player = VideoPlayer::new(true);
        player.push_frame(Duration::from_millis(0), frame(0, 10));
        player.push_frame(Duration::from_millis(40), frame(40, 40));
        player.push_frame(Duration::from_millis(80), frame(80, 80));
        player.set_duration(Some(Duration::from_millis(120)));
        assert_eq!(player.frame_index(), Some(0));
        player.seek(Duration::from_millis(40));
        assert_eq!(player.frame_index(), Some(1));
        player.seek(Duration::from_millis(79));
        assert_eq!(player.frame_index(), Some(1));
        player.seek(Duration::from_millis(80));
        assert_eq!(player.frame_index(), Some(2));
        player.seek(Duration::from_millis(119));
        player.play();
        assert!(player.advance(Duration::from_millis(2)));
        assert_eq!(player.position(), Duration::from_millis(1));
        assert_eq!(player.frame_index(), Some(0));
    }

    #[test]
    fn advancing_reports_end_and_pause() {
        let mut player = VideoPlayer::new(false);
        player.push_frame(Duration::ZERO, frame(0, 10));
        player.push_frame(Duration::from_millis(100), frame(100, 100));
        player.set_duration(Some(Duration::from_millis(100)));
        player.play();
        assert!(player.advance(Duration::from_millis(101)));
        assert!(player.is_ended());
        assert!(!player.is_playing());
        assert_eq!(player.position(), Duration::from_millis(100));
        player.seek(Duration::from_millis(20));
        assert!(!player.is_ended());
        assert_eq!(player.frame_index(), Some(0));
    }

    #[test]
    fn widget_emits_image_geometry_and_placeholder() {
        let size = Size::new(200.0, 120.0);
        let mut context = Context::new(size);
        let mut player = VideoPlayer::new(false);
        context.begin_frame(size);
        let empty = video(&mut context, "video", &player, 160.0, 90.0, VideoStyle::default());
        assert!(empty.placeholder);
        context.produce().unwrap();
        context.end_frame();

        player.push_frame(Duration::ZERO, frame(0, 90));
        player.play();
        context.begin_frame(size);
        let state = video(
            &mut context,
            "video",
            &player,
            160.0,
            90.0,
            VideoStyle {
                fit: ImageFit::Fill,
                ..VideoStyle::default()
            },
        );
        assert!(!state.placeholder);
        let frame = context.produce().unwrap();
        let image = frame
            .display
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Image {
                    width,
                    height,
                    source,
                    ..
                } => Some((*width, *height, source.version())),
                _ => None,
            })
            .expect("image command");
        assert_eq!((image.0, image.1), (160.0, 90.0));
        assert_ne!(image.2, 0);
    }

    #[test]
    fn frame_change_updates_damage() {
        let size = Size::new(200.0, 120.0);
        let mut player = VideoPlayer::new(false);
        player.push_frame(Duration::ZERO, frame(0, 10));
        player.push_frame(Duration::from_millis(40), frame(40, 80));
        player.set_duration(Some(Duration::from_millis(80)));
        player.seek(Duration::ZERO);

        let mut context = Context::new(size);
        context.begin_frame(size);
        video(
            &mut context,
            "video",
            &player,
            160.0,
            90.0,
            VideoStyle {
                fit: ImageFit::Fill,
                ..VideoStyle::default()
            },
        );
        let first = context.produce().unwrap();
        context.end_frame();

        player.seek(Duration::from_millis(40));
        context.begin_frame(size);
        video(
            &mut context,
            "video",
            &player,
            160.0,
            90.0,
            VideoStyle {
                fit: ImageFit::Fill,
                ..VideoStyle::default()
            },
        );
        let second = context.produce().unwrap();
        let damage = crate::render::diff(&first.display, &second.display, crate::render::RenderSize::new(200, 120));
        assert!(!damage.is_empty());
        assert!(damage.is_full() || damage.rects().iter().any(|rect| rect.width > 0.0));
    }
}
