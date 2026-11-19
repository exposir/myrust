//! Font loading, shaping, metrics, and glyph rasterization.
//!
//! Layout uses Rustybuzz for OpenType shaping (kerning, ligatures and script
//! features) and a small ordered fallback stack for missing glyphs. Font
//! discovery is still a fixed path probe rather than fontconfig/CTFont.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
};

use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont};
use anyhow::{anyhow, Result};
use rustybuzz::{Direction, Face, UnicodeBuffer};

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
    bytes: Arc<[u8]>,
    id: u64,
}

/// One positioned glyph produced by OpenType shaping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    /// Font-local glyph id.
    pub glyph_id: u16,
    /// Byte offset of the source cluster within the shaped run.
    pub cluster: usize,
    /// Horizontal advance in logical pixels.
    pub advance: f32,
    /// Horizontal placement offset in logical pixels.
    pub offset_x: f32,
    /// Vertical placement offset in logical pixels.
    pub offset_y: f32,
}

impl FontFace {
    fn next_id() -> u64 {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    }

    /// Stable process-local identifier used by glyph caches.
    pub fn id(&self) -> u64 {
        self.id
    }

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
        // MyGo's browser engine lays text out one pixel taller than the
        // Canvas fontBoundingBox ascent + descent Chrome reports: its 14px
        // radio rows repeat every 25px with an 8px gap, so the line box is
        // 17. Geist's own metrics are 1.30x, which makes every stacked row
        // taller than the page it mirrors.
        normalize_size(size) + 3.0
    }

    /// Returns the ascent above the baseline at the supplied size.
    pub fn ascent(&self, size: f32) -> f32 {
        self.font.as_scaled(self.px_scale(size)).ascent()
    }

    /// Returns the descent below the baseline at the supplied size.
    pub fn descent(&self, size: f32) -> f32 {
        self.font.as_scaled(self.px_scale(size)).descent().abs()
    }

    /// Returns whether the font contains a non-notdef glyph for a character.
    pub fn has_glyph(&self, character: char) -> bool {
        self.font.glyph_id(character).0 != 0
    }

    /// Returns the OpenType glyph id for a character.
    pub fn glyph_id(&self, character: char) -> u16 {
        self.font.glyph_id(character).0
    }

    /// Shapes one bidi/font run with Rustybuzz.
    ///
    /// Positions are in logical pixels, with the project's CSS-size convention
    /// (`size / units_per_em`). The cluster indices are relative to `text`.
    pub fn shape(&self, text: &str, size: f32, right_to_left: bool) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }
        let Some(face) = Face::from_slice(&self.bytes, 0) else {
            return Vec::new();
        };
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        buffer.set_direction(if right_to_left {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        });
        let glyphs = rustybuzz::shape(&face, &[], buffer);
        let units = face.units_per_em().max(1) as f32;
        let scale = normalize_size(size) / units;
        glyphs
            .glyph_infos()
            .iter()
            .zip(glyphs.glyph_positions())
            .map(|(info, position)| ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                cluster: info.cluster as usize,
                advance: position.x_advance as f32 * scale,
                offset_x: position.x_offset as f32 * scale,
                offset_y: position.y_offset as f32 * scale,
            })
            .collect()
    }

    /// Rasterizes one glyph at the supplied pixel size.
    ///
    /// Whitespace and missing glyphs produce a zero-sized bitmap. Missing
    /// glyphs are counted by [`crate::CpuRasterizer`] before this method is
    /// called. Every call rasterizes the glyph again; this batch intentionally
    /// has no glyph cache or subpixel positioning.
    pub fn rasterize(&self, character: char, size: f32) -> GlyphBitmap {
        self.rasterize_glyph_id(self.glyph_id(character), size)
    }

    /// Rasterizes one shaped glyph id at the supplied pixel size.
    pub fn rasterize_glyph_id(&self, glyph_id: u16, size: f32) -> GlyphBitmap {
        if glyph_id == 0 {
            return empty_bitmap();
        }
        let size = normalize_size(size);
        let scale = self.px_scale(size);
        let scaled = self.font.as_scaled(scale);
        let glyph = ab_glyph::GlyphId(glyph_id)
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
        let bytes: Arc<[u8]> = Arc::from(bytes);
        let font = FontVec::try_from_vec(bytes.to_vec())
            .map_err(|_| anyhow!("ab_glyph rejected the font data"))?;
        Ok(FontFace {
            font,
            bytes,
            id: FontFace::next_id(),
        })
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

/// An ordered sans/mono face stack used for shaping and missing-glyph fallback.
#[derive(Debug)]
pub struct FontStack {
    id: u64,
    sans: Arc<FontFace>,
    mono: Option<Arc<FontFace>>,
    fallbacks: Vec<Arc<FontFace>>,
}

impl FontStack {
    /// Builds a stack from one required sans face plus optional mono and
    /// fallback faces.
    pub fn new(
        sans: Arc<FontFace>,
        mono: Option<Arc<FontFace>>,
        fallbacks: Vec<Arc<FontFace>>,
    ) -> Self {
        static NEXT_STACK_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_STACK_ID.fetch_add(1, Ordering::Relaxed),
            sans,
            mono,
            fallbacks,
        }
    }

    /// Stable process-local identifier for layout caches.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Returns the primary sans face.
    pub fn sans(&self) -> &Arc<FontFace> {
        &self.sans
    }

    /// Returns the face that can render `character`.
    pub fn face_for(&self, character: char, mono: bool) -> Option<Arc<FontFace>> {
        if mono {
            if let Some(font) = self.mono.as_ref().filter(|font| font.has_glyph(character)) {
                return Some(font.clone());
            }
            if let Some(font) = self
                .fallbacks
                .iter()
                .find(|font| font.has_glyph(character))
            {
                return Some(font.clone());
            }
        }
        if self.sans.has_glyph(character) {
            return Some(self.sans.clone());
        }
        self.fallbacks
            .iter()
            .find(|font| font.has_glyph(character))
            .cloned()
    }

    /// Returns the primary face used for line metrics.
    pub fn metrics_face(&self, mono: bool) -> &Arc<FontFace> {
        if mono {
            self.mono.as_ref().unwrap_or(&self.sans)
        } else {
            &self.sans
        }
    }
}

/// Loads the process-wide fallback stack, including common CJK and emoji
/// fallbacks.
pub fn default_font_stack() -> Option<Arc<FontStack>> {
    if let Some(stack) = installed_font_stack() {
        return Some(stack);
    }
    if let Some(font) = installed_default_font() {
        return Some(Arc::new(FontStack::new(font, None, Vec::new())));
    }
    static DEFAULT: OnceLock<Option<Arc<FontStack>>> = OnceLock::new();
    DEFAULT
        .get_or_init(|| {
            let sans = Arc::new(FontRegistry::load_first_path(&sans_font_candidates())?);
            let mono = FontRegistry::load_first_path(&mono_font_candidates()).map(Arc::new);
            let mut fallbacks = Vec::new();
            for group in fallback_font_groups() {
                if let Some(font) = group.iter().find_map(|path| FontRegistry::load_path(path)) {
                    if font.bytes.as_ref() != sans.bytes.as_ref()
                        && !fallbacks
                            .iter()
                            .any(|existing: &Arc<FontFace>| {
                                existing.bytes.as_ref() == font.bytes.as_ref()
                            })
                    {
                        fallbacks.push(Arc::new(font));
                    }
                }
            }
            Some(Arc::new(FontStack::new(sans, mono, fallbacks)))
        })
        .clone()
}

/// Returns the process-wide default system font, loading it once on first use.
///
/// An explicitly installed face via [`install_default_font`] takes precedence.
/// Layout measurement and callers of [`default_font`] then share one face.
pub fn default_font() -> Option<Arc<FontFace>> {
    if let Some(font) = installed_default_font() {
        return Some(font);
    }
    if let Some(stack) = installed_font_stack() {
        return Some(stack.sans().clone());
    }
    static DEFAULT: OnceLock<Option<Arc<FontFace>>> = OnceLock::new();
    DEFAULT
        .get_or_init(|| default_font_stack().map(|stack| stack.sans().clone()))
        .clone()
}

static INSTALLED_FONT: OnceLock<Arc<FontFace>> = OnceLock::new();
static INSTALLED_STACK: OnceLock<Arc<FontStack>> = OnceLock::new();

/// Installs a process-wide font used for layout measurement and [`default_font`].
///
/// This is opt-in for environments without system fonts, such as wasm. The
/// first successful call wins; later calls are ignored. Callers that rasterize
/// with an explicit face should install that same face so measured widths match
/// drawn glyph advances.
pub fn install_default_font(font: Arc<FontFace>) -> bool {
    let installed = INSTALLED_FONT.set(font.clone()).is_ok();
    if installed {
        let _ = INSTALLED_STACK.set(Arc::new(FontStack::new(font, None, Vec::new())));
    }
    installed
}

/// Installs a process-wide shaping/fallback stack.
///
/// As with [`install_default_font`], the first successful call wins.
pub fn install_default_font_stack(stack: Arc<FontStack>) -> bool {
    INSTALLED_STACK.set(stack).is_ok()
}

fn installed_default_font() -> Option<Arc<FontFace>> {
    INSTALLED_FONT.get().cloned()
}

fn installed_font_stack() -> Option<Arc<FontStack>> {
    INSTALLED_STACK.get().cloned()
}

/// `TextMeasurer` backed by one real font face.
///
/// Widths use the same OpenType shaping path as the renderers. Fallback and
/// bidi are provided by [`FontStack`]; this measurer is the single-face form.
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
                self.font
                    .shape(line, font_size, false)
                    .iter()
                    .map(|glyph| glyph.advance)
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
    sans_font_candidates()
}

fn sans_font_candidates() -> Vec<PathBuf> {
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

fn mono_font_candidates() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
        "/System/Library/Fonts/Menlo.ttc",
        "/System/Library/Fonts/Supplemental/Courier New.ttf",
        r"C:\Windows\Fonts\consola.ttf",
        r"C:\Windows\Fonts\cour.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

fn fallback_font_groups() -> Vec<Vec<PathBuf>> {
    [
        &[
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/usr/share/fonts/truetype/arphic/uming.ttc",
            "/System/Library/Fonts/PingFang.ttc",
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\meiryo.ttc",
        ][..],
        &[
            "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
            "/usr/share/fonts/truetype/noto/NotoEmoji-Regular.ttf",
            "/System/Library/Fonts/Apple Color Emoji.ttc",
            r"C:\Windows\Fonts\seguiemj.ttf",
        ][..],
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            r"C:\Windows\Fonts\seguisym.ttf",
        ][..],
    ]
    .into_iter()
    .map(|group| group.iter().map(PathBuf::from).collect())
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
