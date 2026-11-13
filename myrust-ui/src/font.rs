//! Single-face TTF/OpenType loading, metrics, and glyph rasterization.
//!
//! This module uses `ab_glyph` and intentionally performs no shaping, kerning,
//! ligature substitution, bidi reordering, or font fallback. It probes a small
//! fixed list of common system font paths; it is not a full font-discovery
//! implementation.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont};
use anyhow::{anyhow, Result};

use super::text::{TextMeasurer, TextMetrics};

/// A rasterized glyph coverage bitmap.
///
/// Pixels are stored row-major as 8-bit coverage values, where 0 is fully
/// transparent and 255 is fully opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphBitmap {
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// Horizontal offset from the pen position to the bitmap's left edge.
    pub bearing_x: i32,
    /// Vertical offset from the baseline to the bitmap's top edge.
    pub bearing_y: i32,
    /// Row-major 8-bit coverage values.
    pub coverage: Vec<u8>,
}

/// One loaded font face.
#[derive(Debug)]
pub struct FontFace {
    font: FontVec,
}

impl FontFace {
    /// Converts a CSS `font-size` (an em size) into the pixel-height scale
    /// expected by `ab_glyph`. CSS sizes are em sizes, while `PxScale` is the
    /// font's ascent-to-descent height; without this conversion a 14px font
    /// measures and rasterizes about 23% smaller than canvas2D.
    fn px_scale(&self, size: f32) -> PxScale {
        let size = normalize_size(size);
        let units = self.font.units_per_em().unwrap_or(size);
        let height = self.font.height_unscaled();
        if units > 0.0 && height > 0.0 {
            PxScale::from(size * height / units)
        } else {
            PxScale::from(size)
        }
    }

    /// Returns the horizontal advance for a character at the supplied size.
    pub fn glyph_advance(&self, character: char, size: f32) -> f32 {
        if !self.has_glyph(character) {
            return 0.0;
        }
        let scaled = self.font.as_scaled(self.px_scale(size));
        scaled.h_advance(scaled.glyph_id(character))
    }

    /// Returns the recommended line height at the supplied size.
    pub fn line_height(&self, size: f32) -> f32 {
        let scaled = self.font.as_scaled(self.px_scale(size));
        let size = normalize_size(size);
        (scaled.ascent() - scaled.descent() + scaled.line_gap()).max(size)
    }

    /// Returns the ascent above the baseline at the supplied size.
    pub fn ascent(&self, size: f32) -> f32 {
        self.font.as_scaled(self.px_scale(size)).ascent()
    }

    /// Returns whether the font contains a non-notdef glyph for a character.
    pub fn has_glyph(&self, character: char) -> bool {
        self.font.glyph_id(character).0 != 0
    }

    /// Rasterizes one glyph at the supplied pixel size.
    ///
    /// Whitespace and missing glyphs produce a zero-sized bitmap. Missing
    /// glyphs are counted by [`crate::CpuRasterizer`] before this method is
    /// called. Every call rasterizes the glyph again; this batch intentionally
    /// has no glyph cache or subpixel positioning.
    pub fn rasterize(&self, character: char, size: f32) -> GlyphBitmap {
        if !self.has_glyph(character) {
            return empty_bitmap();
        }
        let size = normalize_size(size);
        let scale = self.px_scale(size);
        let scaled = self.font.as_scaled(scale);
        let glyph = scaled
            .glyph_id(character)
            .with_scale_and_position(scale, point(0.0, 0.0));
        let Some(outlined) = scaled.outline_glyph(glyph) else {
            return empty_bitmap();
        };
        let bounds = outlined.px_bounds();
        let width = bounds.width().max(0.0).round() as u32;
        let height = bounds.height().max(0.0).round() as u32;
        if width == 0 || height == 0 {
            return empty_bitmap();
        }
        let mut coverage = vec![0_u8; width as usize * height as usize];
        outlined.draw(|x, y, value| {
            if x < width && y < height {
                coverage[y as usize * width as usize + x as usize] =
                    (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        });
        GlyphBitmap {
            width,
            height,
            bearing_x: bounds.min.x.floor() as i32,
            bearing_y: bounds.min.y.floor() as i32,
            coverage,
        }
    }
}

/// Loader and small path-probing registry for system fonts.
#[derive(Debug, Default, Clone, Copy)]
pub struct FontRegistry;

impl FontRegistry {
    /// Loads one owned font file from bytes.
    pub fn load_bytes(bytes: Vec<u8>) -> Result<FontFace> {
        let font =
            FontVec::try_from_vec(bytes).map_err(|_| anyhow!("ab_glyph rejected the font data"))?;
        Ok(FontFace { font })
    }

    /// Loads the first available font from a small common-path probe list.
    ///
    /// This does not enumerate installed fonts or parse font configuration.
    pub fn load_system_default() -> Option<FontFace> {
        Self::load_first_path(&system_font_candidates())
    }

    fn load_first_path(paths: &[PathBuf]) -> Option<FontFace> {
        for path in paths {
            if let Some(font) = Self::load_path(path) {
                return Some(font);
            }
        }
        None
    }

    fn load_path(path: &Path) -> Option<FontFace> {
        let bytes = fs::read(path).ok()?;
        Self::load_bytes(bytes).ok()
    }
}

/// Returns the process-wide default system font, loading it once on first use.
///
/// An explicitly installed face via [`install_default_font`] takes precedence.
/// Layout measurement and callers of [`default_font`] then share one face.
pub fn default_font() -> Option<Arc<FontFace>> {
    if let Some(font) = installed_default_font() {
        return Some(font);
    }
    static DEFAULT: OnceLock<Option<Arc<FontFace>>> = OnceLock::new();
    DEFAULT
        .get_or_init(|| FontRegistry::load_system_default().map(Arc::new))
        .clone()
}

static INSTALLED_FONT: OnceLock<Arc<FontFace>> = OnceLock::new();

/// Installs a process-wide font used for layout measurement and [`default_font`].
///
/// This is opt-in for environments without system fonts, such as wasm. The
/// first successful call wins; later calls are ignored. Callers that rasterize
/// with an explicit face should install that same face so measured widths match
/// drawn glyph advances.
pub fn install_default_font(font: Arc<FontFace>) -> bool {
    INSTALLED_FONT.set(font).is_ok()
}

fn installed_default_font() -> Option<Arc<FontFace>> {
    INSTALLED_FONT.get().cloned()
}

/// `TextMeasurer` backed by one real font face.
///
/// Widths are the sum of unshaped glyph advances. Kerning, shaping, fallback,
/// and bidi are intentionally absent.
pub struct RealFontMeasurer<'a> {
    font: &'a FontFace,
}

impl<'a> RealFontMeasurer<'a> {
    /// Creates a real-font measurer for one face.
    pub fn new(font: &'a FontFace) -> Self {
        Self { font }
    }
}

impl TextMeasurer for RealFontMeasurer<'_> {
    fn measure(&self, text: &str, font_size: f32) -> TextMetrics {
        let font_size = normalize_size(font_size);
        let line_height = self.font.line_height(font_size);
        let lines = text.split('\n').collect::<Vec<_>>();
        let width = lines
            .iter()
            .map(|line| {
                line.chars()
                    .map(|character| self.font.glyph_advance(character, font_size))
                    .sum::<f32>()
            })
            .fold(0.0_f32, f32::max);
        TextMetrics {
            width,
            height: lines.len().max(1) as f32 * line_height,
            line_height,
        }
    }
}

fn system_font_candidates() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial.ttf",
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\arial.ttf",
        r"C:\Windows\Fonts\Helvetica.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

fn normalize_size(size: f32) -> f32 {
    if size.is_finite() {
        size.max(1.0)
    } else {
        1.0
    }
}

fn empty_bitmap() -> GlyphBitmap {
    GlyphBitmap {
        width: 0,
        height: 0,
        bearing_x: 0,
        bearing_y: 0,
        coverage: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{layout_text, ApproximateMeasurer};

    const DEJAVU: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

    fn load_dejavu() -> Option<FontFace> {
        FontRegistry::load_path(Path::new(DEJAVU))
    }

    #[test]
    fn real_font_renders_expected_advance() {
        let Some(font) = load_dejavu() else {
            return;
        };
        let measurer = RealFontMeasurer::new(&font);
        let one = measurer.measure("M", 32.0).width;
        let two = measurer.measure("MM", 32.0).width;
        assert!((10.0..40.0).contains(&one), "unexpected advance {one}");
        assert!(
            (two - one * 2.0).abs() < one * 0.2,
            "two-glyph width {two} should be about twice {one}"
        );
    }

    #[test]
    fn real_font_glyph_has_coverage() {
        let Some(font) = load_dejavu() else {
            return;
        };
        let glyph = font.rasterize('M', 32.0);
        assert!(glyph.width > 0);
        assert!(glyph.height > 0);
        assert!(glyph.coverage.iter().any(|coverage| *coverage > 0));
    }

    #[test]
    fn real_font_measurement_differs_from_approximation() {
        let Some(font) = load_dejavu() else {
            return;
        };
        let text = "MMMM";
        let real = RealFontMeasurer::new(&font).measure(text, 32.0);
        let approximate = ApproximateMeasurer.measure(text, 32.0);
        assert!((real.width - approximate.width).abs() > 1.0);
    }

    #[test]
    fn real_font_measurer_drives_text_layout() {
        let Some(font) = load_dejavu() else {
            return;
        };
        let measurer = RealFontMeasurer::new(&font);
        let lines = layout_text("MMMM", 32.0, None, &measurer);
        assert_eq!(lines.len(), 1);
        let expected = font.glyph_advance('M', 32.0) * 4.0;
        assert!((lines[0].width - expected).abs() < 0.1);
        assert_eq!(lines[0].line_height, font.line_height(32.0));
    }

    #[test]
    fn missing_font_path_returns_none_without_panicking() {
        let missing = PathBuf::from("/definitely/missing/myrust-font.ttf");
        assert!(FontRegistry::load_first_path(&[missing]).is_none());
    }

    #[test]
    fn missing_glyph_reports_zero_advance_and_empty_bitmap() {
        let Some(font) = load_dejavu() else {
            return;
        };
        let character = '\u{10FFFF}';
        assert!(!font.has_glyph(character));
        assert_eq!(font.glyph_advance(character, 24.0), 0.0);
        let bitmap = font.rasterize(character, 24.0);
        assert_eq!(bitmap.width, 0);
        assert_eq!(bitmap.height, 0);
    }
}
