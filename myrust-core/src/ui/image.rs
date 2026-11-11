//! RGBA image drawing contract.
//!
//! The framework does not decode PNG/JPEG or any other file format. The caller
//! provides a decoded RGBA8 buffer. This control scales it to the target
//! rectangle with nearest-neighbor sampling and emits ordinary filled rects,
//! so the existing CPU presenter can draw it without a new command variant.

use super::context::Context;

/// Decoded RGBA8 source pixels.
#[derive(Debug, Clone, Copy)]
pub struct ImageData<'a> {
    /// RGBA8 pixels, row-major, four bytes per pixel.
    pub pixels: &'a [u8],
    /// Source width in pixels.
    pub width: u32,
    /// Source height in pixels.
    pub height: u32,
}

impl<'a> ImageData<'a> {
    /// Creates image data from a decoded RGBA8 buffer.
    pub const fn new(pixels: &'a [u8], width: u32, height: u32) -> Self {
        Self {
            pixels,
            width,
            height,
        }
    }

    /// Whether the buffer has enough bytes for the declared size.
    pub fn is_valid(&self) -> bool {
        if self.width == 0 || self.height == 0 {
            return false;
        }
        self.width
            .checked_mul(self.height)
            .and_then(|pixels| pixels.checked_mul(4))
            .is_some_and(|required| self.pixels.len() >= required as usize)
    }
}

/// Result of drawing an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImageState {
    /// Whether a valid image with a positive target was drawn.
    pub drawn: bool,
    /// Logical target width.
    pub width: u32,
    /// Logical target height.
    pub height: u32,
    /// Number of sampled output columns.
    pub columns: usize,
    /// Number of sampled output rows.
    pub rows: usize,
    /// Declared source width.
    pub source_width: u32,
    /// Declared source height.
    pub source_height: u32,
}

/// Draws decoded RGBA8 pixels scaled into a target rectangle.
///
/// `target_width`/`target_height` are logical pixels. Zero or non-finite
/// targets emit no pixels and report `drawn = false`.
pub fn image(
    context: &mut Context,
    id: &str,
    data: ImageData<'_>,
    target_width: f32,
    target_height: f32,
) -> ImageState {
    context.image(id, data, target_width, target_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::layout::Size;
    use crate::ui::paint::{Color, DrawCommand};

    #[test]
    fn image_scales_rgba_pixels_and_handles_zero_target() {
        let pixels = [
            255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let data = ImageData::new(&pixels, 2, 2);
        let size = Size::new(80.0, 80.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let drawn = image(&mut context, "image", data, 4.0, 4.0);
        let zero = image(&mut context, "image-zero", data, 0.0, 0.0);
        let invalid = image(
            &mut context,
            "image-invalid",
            ImageData::new(&pixels[..8], 2, 2),
            4.0,
            4.0,
        );
        let frame = context.produce().unwrap();
        assert!(drawn.drawn);
        assert_eq!(drawn.columns, 4);
        assert_eq!(drawn.rows, 4);
        let rects = frame
            .display
            .commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::Rect { .. }))
            .count();
        assert_eq!(rects, 16);
        assert!(frame.display.commands.iter().any(|command| {
            matches!(command, DrawCommand::Rect { x, y, color, .. }
                if *x == 0.0 && *y == 0.0 && *color == Color::rgb(255, 0, 0))
        }));
        assert!(frame.display.commands.iter().any(|command| {
            matches!(command, DrawCommand::Rect { x, y, color, .. }
                if *x == 3.0 && *y == 3.0 && *color == Color::rgb(255, 255, 255))
        }));
        assert!(!zero.drawn);
        assert_eq!(zero.width, 0);
        assert_eq!(zero.height, 0);
        assert!(!invalid.drawn);
    }
}
