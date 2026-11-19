//! Small CPU presenter for native windows.
//!
//! This is intentionally separate from [`crate::SoftwareRenderer`],
//! which remains a non-rasterizing command analyzer. The presenter draws basic
//! geometry and prefers a system TTF through `ab_glyph`, falling back to a
//! fixed 8x8 ASCII bitmap font. It is not a general shaping engine or image
//! decoder.

use font8x8::{UnicodeFonts, BASIC_FONTS};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
};

use super::font::{default_font_stack, FontFace, FontStack, GlyphBitmap};
use super::image::ImageHandle;
use super::layout::Point;
use super::paint::{
    Color, DisplayList, DrawCommand, GradientStop, ImageSourceRect, Radius, Rect, TextAlignment,
};
use super::render::{
    color_to_array, command_device_bounds, intersect_rects, CommandCounts, Damage, RenderSize,
    TransformMatrix,
};
use super::svg::{parse_svg_path, SvgContour};
use super::text::{layout_text_with_stack, TextLayout};

const DEFAULT_GLYPH_CACHE_CAPACITY: usize = 4096;

/// CPU glyph bitmap cache counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GlyphCacheStats {
    /// Bitmap reuse count.
    pub hits: u64,
    /// Bitmap rasterization count.
    pub misses: u64,
    /// Current entry count.
    pub entries: usize,
}

#[derive(Hash, PartialEq, Eq)]
struct GlyphCacheKey {
    stack_id: u64,
    face_id: u64,
    glyph_id: u16,
    size: u32,
    bold: bool,
    italic: bool,
}

static GLYPH_CACHE: OnceLock<Mutex<HashMap<GlyphCacheKey, GlyphBitmap>>> = OnceLock::new();
static GLYPH_HITS: AtomicU64 = AtomicU64::new(0);
static GLYPH_MISSES: AtomicU64 = AtomicU64::new(0);
static GLYPH_CACHE_ENABLED: AtomicBool = AtomicBool::new(true);
static GLYPH_CACHE_CAPACITY: AtomicUsize = AtomicUsize::new(DEFAULT_GLYPH_CACHE_CAPACITY);

/// Returns CPU glyph bitmap cache counters.
pub fn glyph_cache_stats() -> GlyphCacheStats {
    let entries = GLYPH_CACHE
        .get()
        .and_then(|cache| cache.lock().ok().map(|cache| cache.len()))
        .unwrap_or(0);
    GlyphCacheStats {
        hits: GLYPH_HITS.load(Ordering::Relaxed),
        misses: GLYPH_MISSES.load(Ordering::Relaxed),
        entries,
    }
}

/// Enables or disables the CPU glyph bitmap cache for comparison tests.
pub fn set_glyph_cache_enabled(enabled: bool) {
    GLYPH_CACHE_ENABLED.store(enabled, Ordering::Relaxed);
}

#[cfg(test)]
fn set_glyph_cache_capacity(capacity: usize) {
    GLYPH_CACHE_CAPACITY.store(capacity.max(1), Ordering::Relaxed);
}

fn cached_glyph(
    stack_id: u64,
    font: &FontFace,
    glyph_id: u16,
    size: f32,
    bold: bool,
    italic: bool,
) -> GlyphBitmap {
    let key = GlyphCacheKey {
        stack_id,
        face_id: font.id(),
        glyph_id,
        size: size.max(1.0).to_bits(),
        bold,
        italic,
    };
    let enabled = GLYPH_CACHE_ENABLED.load(Ordering::Relaxed);
    let cache = GLYPH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if enabled {
        if let Ok(cache) = cache.lock() {
            if let Some(bitmap) = cache.get(&key) {
                GLYPH_HITS.fetch_add(1, Ordering::Relaxed);
                return bitmap.clone();
            }
        }
    }
    let bitmap = font.rasterize_glyph_id(glyph_id, size);
    if enabled {
        GLYPH_MISSES.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut cache) = cache.lock() {
            let capacity = GLYPH_CACHE_CAPACITY.load(Ordering::Relaxed).max(1);
            if cache.len() >= capacity {
                cache.clear();
            }
            cache.insert(key, bitmap.clone());
        }
    }
    bitmap
}

/// A collected text command for browser-native canvas2D rendering.
#[derive(Debug, Clone)]
pub struct TextRenderCommand {
    /// The x coordinate (adjusted for alignment within max_width).
    pub x: f32,
    /// The y coordinate (top of this line).
    pub y: f32,
    /// The text to render.
    pub text: String,
    /// Font size.
    pub font_size: f32,
    /// Font weight (400 normal, 700 bold).
    pub weight: u16,
    /// Whether the run is drawn in the italic face.
    pub italic: bool,
    /// Whether the run is drawn in the monospaced face.
    pub mono: bool,
    /// Named font family, as MyGo's `Font(name)` takes it.
    pub family: Option<String>,
    /// Text color.
    pub color: Color,
    /// Maximum width for alignment, or None.
    pub max_width: Option<f32>,
    /// Text alignment.
    pub alignment: TextAlignment,
    /// Accumulated 2D affine matrix [m11, m12, m21, m22, tx, ty].
    pub transform: [f32; 6],
    /// Raster-space clip rectangle `[x, y, width, height]`, if any.
    pub clip: Option<[f32; 4]>,
}

/// Error produced while rasterizing a display list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RasterError {
    /// A zero-sized pixel buffer cannot be created.
    #[error("CPU raster target must be non-zero, got {width}x{height}")]
    InvalidSize {
        /// Invalid width.
        width: u32,
        /// Invalid height.
        height: u32,
    },
}

/// Packed CPU pixel buffer.
///
/// Pixels use `0x00RRGGBB`, matching the native software presentation buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuFrame {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Packed RGB pixels in row-major order.
    pub pixels: Vec<u32>,
    /// Command counts consumed while drawing.
    pub command_counts: CommandCounts,
    /// Image commands whose source carried no decoded pixels.
    pub unsupported_commands: usize,
    /// Glyphs missing from the active real font or bitmap fallback.
    pub unsupported_glyphs: usize,
    /// Pixels touched by the last rasterization pass.
    pub rasterized_pixels: u64,
    /// Glyph bitmap cache hits in this pass.
    pub glyph_cache_hits: u64,
    /// Glyph bitmap cache misses in this pass.
    pub glyph_cache_misses: u64,
}

impl CpuFrame {
    /// Returns a mutable row-major pixel slice.
    pub fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.pixels
    }
}

/// CPU rasterizer for basic display-list geometry.
#[derive(Debug, Default, Clone, Copy)]
pub struct CpuRasterizer;

impl CpuRasterizer {
    /// Rasterizes a display list into a packed RGB pixel buffer.
    ///
    /// Supported: rectangles, rounded rectangles, circles, lines, linear
    /// gradients, SVG path fills/strokes, decoded images, transforms, and
    /// scissors. Text uses the process-wide TTF font when available and falls
    /// back to the built-in 8x8 bitmap font. Text layout, shaping, bidi
    /// ordering and fallback come from the shared
    /// [`TextLayout`](crate::text::TextLayout). Decoded images use bilinear
    /// sampling and alpha blending; handles without pixels are counted as
    /// unsupported.
    pub fn rasterize(
        display_list: &DisplayList,
        size: RenderSize,
        background: Color,
    ) -> Result<CpuFrame, RasterError> {
        Self::rasterize_inner(
            display_list,
            size,
            background,
            default_font_stack(),
            false,
            None,
            None,
        )
        .map(|(frame, _)| frame)
    }

    /// Rasterizes with an explicit font choice.
    ///
    /// Passing `None` forces the 8x8 bitmap fallback and is useful for tests or
    /// headless environments where no system font should be loaded.
    pub fn rasterize_with_font(
        display_list: &DisplayList,
        size: RenderSize,
        background: Color,
        font: Option<Arc<FontFace>>,
    ) -> Result<CpuFrame, RasterError> {
        let stack = font.map(|font| Arc::new(FontStack::new(font, None, Vec::new())));
        Self::rasterize_inner(display_list, size, background, stack, false, None, None)
            .map(|(frame, _)| frame)
    }

    /// Like [`rasterize_with_font`] but skips text drawing and returns collected
    /// text commands for browser-native canvas2D rendering.
    pub fn rasterize_with_font_collect_text(
        display_list: &DisplayList,
        size: RenderSize,
        background: Color,
        font: Option<Arc<FontFace>>,
    ) -> Result<(CpuFrame, Vec<TextRenderCommand>), RasterError> {
        let stack = font.map(|font| Arc::new(FontStack::new(font, None, Vec::new())));
        Self::rasterize_inner(display_list, size, background, stack, true, None, None)
    }

    /// Reuses `previous` pixels and redraws only `damage` rectangles.
    ///
    /// A size change, background/font-stack change represented by `damage.full`,
    /// or an empty damage set is handled before this method; callers should
    /// compare backgrounds and font generations and request `Damage::full()`.
    pub fn rasterize_damaged(
        previous: &CpuFrame,
        display_list: &DisplayList,
        size: RenderSize,
        background: Color,
        damage: &Damage,
    ) -> Result<CpuFrame, RasterError> {
        Self::rasterize_damaged_inner(
            previous,
            display_list,
            size,
            background,
            damage,
            true,
        )
    }

    fn rasterize_damaged_inner(
        previous: &CpuFrame,
        display_list: &DisplayList,
        size: RenderSize,
        background: Color,
        damage: &Damage,
        prefilter: bool,
    ) -> Result<CpuFrame, RasterError> {
        if previous.width != size.width
            || previous.height != size.height
            || damage.is_full()
        {
            return Self::rasterize(display_list, size, background);
        }
        if damage.is_empty() {
            let mut frame = previous.clone();
            frame.command_counts = CommandCounts::default();
            frame.rasterized_pixels = 0;
            frame.glyph_cache_hits = 0;
            frame.glyph_cache_misses = 0;
            return Ok(frame);
        }
        let mut pixels = previous.pixels.clone();
        let mut total_counts = CommandCounts::default();
        let mut rasterized_pixels = 0_u64;
        let mut unsupported_commands = 0_usize;
        let mut unsupported_glyphs = 0_usize;
        let mut glyph_cache_hits = 0_u64;
        let mut glyph_cache_misses = 0_u64;
        for rect in damage.rects() {
            clear_pixels(&mut pixels, size.width, *rect, background);
            let filtered;
            let display = if prefilter {
                filtered = prefilter_display(display_list, *rect);
                &filtered
            } else {
                display_list
            };
            let (frame, _) = Self::rasterize_inner(
                display,
                size,
                background,
                default_font_stack(),
                false,
                Some(pixels),
                Some(*rect),
            )?;
            pixels = frame.pixels;
            add_counts(&mut total_counts, frame.command_counts);
            rasterized_pixels += frame.rasterized_pixels;
            unsupported_commands += frame.unsupported_commands;
            unsupported_glyphs += frame.unsupported_glyphs;
            glyph_cache_hits += frame.glyph_cache_hits;
            glyph_cache_misses += frame.glyph_cache_misses;
        }
        Ok(CpuFrame {
            width: size.width,
            height: size.height,
            pixels,
            command_counts: total_counts,
            unsupported_commands,
            unsupported_glyphs,
            rasterized_pixels,
            glyph_cache_hits,
            glyph_cache_misses,
        })
    }

    fn rasterize_inner(
        display_list: &DisplayList,
        size: RenderSize,
        background: Color,
        stack: Option<Arc<FontStack>>,
        collect_text: bool,
        pixels: Option<Vec<u32>>,
        base_scissor: Option<Rect>,
    ) -> Result<(CpuFrame, Vec<TextRenderCommand>), RasterError> {
        if size.is_empty() {
            return Err(RasterError::InvalidSize {
                width: size.width,
                height: size.height,
            });
        }
        let glyph_stats_before = glyph_cache_stats();
        let mut canvas = Canvas::new(size, background, stack);
        if let Some(pixels) = pixels {
            canvas.pixels = pixels;
        }
        canvas.collect_text_only = collect_text;
        let mut transform = TransformMatrix::IDENTITY;
        let mut transform_stack = Vec::new();
        let mut scissor = base_scissor;
        let mut scissor_stack = Vec::new();

        for command in &display_list.commands {
            canvas.command_counts.record(command);
            match command {
                DrawCommand::TransformPush {
                    transform: local, ..
                } => {
                    transform_stack.push(transform);
                    transform = transform.then(TransformMatrix::from_transform(*local));
                }
                DrawCommand::TransformPop => {
                    if let Some(previous) = transform_stack.pop() {
                        transform = previous;
                    }
                }
                DrawCommand::ScissorPush { rect } => {
                    scissor_stack.push(scissor);
                    let transformed = transform.transform_rect(*rect);
                    scissor = Some(match scissor {
                        Some(current) => intersect_rects(current, transformed),
                        None => transformed,
                    });
                }
                DrawCommand::ScissorPop => {
                    if let Some(previous) = scissor_stack.pop() {
                        scissor = previous;
                    }
                }
                DrawCommand::Text {
                    x,
                    y,
                    text,
                    font_size,
                    weight,
                    italic,
                    mono,
                    family,
                    color,
                    max_width,
                    alignment,
                } => {
                    let layout = canvas.stack.as_ref().and_then(|stack| {
                        layout_text_with_stack(
                            stack,
                            text,
                            *font_size,
                            *max_width,
                            *alignment,
                            *mono,
                        )
                    });
                    if collect_text {
                        if let Some(layout) = layout.as_deref() {
                            canvas.unsupported_glyphs += layout.unsupported_glyphs;
                            for (index, line) in layout.lines.iter().enumerate() {
                                canvas.text_commands.push(TextRenderCommand {
                                    x: *x + line.x_offset,
                                    y: *y + index as f32 * layout.line_height,
                                    text: line.text.clone(),
                                    font_size: *font_size,
                                    weight: *weight,
                                    italic: *italic,
                                    mono: *mono,
                                    family: family.clone(),
                                    color: *color,
                                    max_width: None,
                                    alignment: TextAlignment::Start,
                                    transform: transform.to_array(),
                                    clip: scissor.map(|rect| {
                                        [rect.x, rect.y, rect.width, rect.height]
                                    }),
                                });
                            }
                        } else {
                            let line_height = canvas.line_height(*font_size);
                            for (index, line) in text.split('\n').enumerate() {
                                canvas.text_commands.push(TextRenderCommand {
                                    x: *x,
                                    y: *y + index as f32 * line_height,
                                    text: line.to_string(),
                                    font_size: *font_size,
                                    weight: *weight,
                                    italic: *italic,
                                    mono: *mono,
                                    family: family.clone(),
                                    color: *color,
                                    max_width: None,
                                    alignment: TextAlignment::Start,
                                    transform: transform.to_array(),
                                    clip: scissor.map(|rect| {
                                        [rect.x, rect.y, rect.width, rect.height]
                                    }),
                                });
                            }
                        }
                    } else if let Some(layout) = layout.as_deref() {
                        canvas.unsupported_glyphs += layout.unsupported_glyphs;
                        canvas.draw_text_layout(
                            transform,
                            scissor,
                            Point::new(*x, *y),
                            layout,
                            *weight,
                            *italic,
                            *color,
                        );
                    } else {
                        canvas.draw_text(
                            transform,
                            scissor,
                            Point::new(*x, *y),
                            text,
                            *font_size,
                            *weight,
                            *italic,
                            *mono,
                            family.as_deref(),
                            *color,
                        );
                    }
                }
                DrawCommand::SvgPath {
                    path,
                    fill,
                    stroke,
                    stroke_width,
                    ..
                } => {
                    let outline = parse_svg_path(path);
                    if let Some(fill) = fill {
                        canvas.fill_contours(transform, scissor, &outline.contours, *fill);
                    }
                    if let Some(stroke) = stroke {
                        canvas.stroke_contours(
                            transform,
                            scissor,
                            &outline.contours,
                            *stroke_width,
                            *stroke,
                        );
                    }
                }
                DrawCommand::Image {
                    x,
                    y,
                    width,
                    height,
                    source,
                    source_rect,
                    radius,
                    grayscale,
                } => {
                    canvas.draw_image(
                        transform,
                        scissor,
                        Rect::new(*x, *y, *width, *height),
                        source,
                        *source_rect,
                        *radius,
                        *grayscale,
                    );
                }
                _ => match command {
                    DrawCommand::Rect {
                        x,
                        y,
                        width,
                        height,
                        color,
                    } => canvas.fill_rect(
                        transform,
                        scissor,
                        Rect::new(*x, *y, *width, *height),
                        *color,
                    ),
                    DrawCommand::RoundedRect {
                        x,
                        y,
                        width,
                        height,
                        radius,
                        color,
                    } => canvas.fill_rounded_rect(
                        transform,
                        scissor,
                        Rect::new(*x, *y, *width, *height),
                        *radius,
                        *color,
                    ),
                    DrawCommand::Circle {
                        x,
                        y,
                        radius,
                        color,
                    } => {
                        canvas.fill_circle(transform, scissor, Point::new(*x, *y), *radius, *color)
                    }
                    DrawCommand::Line {
                        x1,
                        y1,
                        x2,
                        y2,
                        width,
                        color,
                    } => {
                        let scale = transform.scale_factor();
                        canvas.draw_line_device(
                            scissor,
                            transform.apply(Point::new(*x1, *y1)),
                            transform.apply(Point::new(*x2, *y2)),
                            *width * scale,
                            *color,
                        )
                    }
                    DrawCommand::Shadow {
                        x,
                        y,
                        width,
                        height,
                        radius,
                        offset_x,
                        offset_y,
                        blur,
                        color,
                    } => canvas.fill_shadow(
                        transform,
                        scissor,
                        Rect::new(*x, *y, *width, *height),
                        *radius,
                        Point::new(*offset_x, *offset_y),
                        *blur,
                        *color,
                    ),
                    DrawCommand::LinearGradient {
                        x,
                        y,
                        width,
                        height,
                        radius,
                        start,
                        end,
                        stops,
                    } => canvas.fill_gradient(
                        transform,
                        scissor,
                        Rect::new(*x, *y, *width, *height),
                        *radius,
                        *start,
                        *end,
                        stops,
                    ),
                    _ => {}
                },
            }
        }

        let text_commands = std::mem::take(&mut canvas.text_commands);
        let unsupported_commands = canvas.unsupported_images;
        let glyph_stats_after = glyph_cache_stats();
        Ok((
            CpuFrame {
                width: size.width,
                height: size.height,
                pixels: canvas.pixels,
                command_counts: canvas.command_counts,
                unsupported_commands,
                unsupported_glyphs: canvas.unsupported_glyphs,
                rasterized_pixels: base_scissor.map_or(
                    u64::from(size.width) * u64::from(size.height),
                    |rect| {
                        let width = rect.width.max(0.0).min(size.width as f32) as u64;
                        let height = rect.height.max(0.0).min(size.height as f32) as u64;
                        width * height
                    },
                ),
                glyph_cache_hits: glyph_stats_after
                    .hits
                    .saturating_sub(glyph_stats_before.hits),
                glyph_cache_misses: glyph_stats_after
                    .misses
                    .saturating_sub(glyph_stats_before.misses),
            },
            text_commands,
        ))
    }
}

struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
    command_counts: CommandCounts,
    unsupported_glyphs: usize,
    unsupported_images: usize,
    font: Option<Arc<FontFace>>,
    stack: Option<Arc<FontStack>>,
    text_commands: Vec<TextRenderCommand>,
    collect_text_only: bool,
}

impl Canvas {
    fn new(size: RenderSize, background: Color, stack: Option<Arc<FontStack>>) -> Self {
        let packed = pack_color(background);
        let font = stack.as_ref().map(|stack| stack.sans().clone());
        Self {
            width: size.width,
            height: size.height,
            pixels: vec![packed; size.width as usize * size.height as usize],
            command_counts: CommandCounts::default(),
            unsupported_glyphs: 0,
            unsupported_images: 0,
            font,
            stack,
            text_commands: Vec::new(),
            collect_text_only: false,
        }
    }

    fn fill_rect(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        rect: Rect,
        color: Color,
    ) {
        let corners = [
            transform.apply(Point::new(rect.x, rect.y)),
            transform.apply(Point::new(rect.right(), rect.y)),
            transform.apply(Point::new(rect.right(), rect.bottom())),
            transform.apply(Point::new(rect.x, rect.bottom())),
        ];
        if (corners[0].y - corners[1].y).abs() < 0.001
            && (corners[1].x - corners[2].x).abs() < 0.001
            && (corners[2].y - corners[3].y).abs() < 0.001
            && (corners[3].x - corners[0].x).abs() < 0.001
        {
            let left = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
            let top = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let right = corners.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
            let bottom = corners.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
            self.fill_axis_aligned_rect(scissor, Rect::new(left, top, right - left, bottom - top), color);
            return;
        }
        let color = color_to_array(color);
        self.fill_triangle(
            transform, scissor, corners[0], corners[1], corners[2], [color; 3],
        );
        self.fill_triangle(
            transform, scissor, corners[0], corners[2], corners[3], [color; 3],
        );
    }

    fn fill_axis_aligned_rect(&mut self, scissor: Option<Rect>, rect: Rect, color: Color) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let color = color_to_array(color);
        let mut min_x = rect.x.floor().max(0.0) as u32;
        let mut min_y = rect.y.floor().max(0.0) as u32;
        let mut max_x = rect.right().ceil().min(self.width as f32) as u32;
        let mut max_y = rect.bottom().ceil().min(self.height as f32) as u32;
        if let Some(clip) = scissor {
            min_x = min_x.max(clip.x.max(0.0).floor() as u32);
            min_y = min_y.max(clip.y.max(0.0).floor() as u32);
            max_x = max_x.min(clip.right().ceil().clamp(0.0, self.width as f32) as u32);
            max_y = max_y.min(clip.bottom().ceil().clamp(0.0, self.height as f32) as u32);
        }
        if min_x >= max_x || min_y >= max_y {
            return;
        }
        for y in min_y..max_y {
            for x in min_x..max_x {
                let overlap_x = (rect.right().min(x as f32 + 1.0) - rect.x.max(x as f32)).max(0.0);
                let overlap_y = (rect.bottom().min(y as f32 + 1.0) - rect.y.max(y as f32)).max(0.0);
                let coverage = overlap_x * overlap_y;
                if coverage > 0.0 {
                    let mut blended = color;
                    blended[3] *= coverage;
                    self.blend(x, y, blended);
                }
            }
        }
    }

    fn fill_axis_aligned_rounded_rect(
        &mut self,
        scissor: Option<Rect>,
        rect: Rect,
        radii: [f32; 4],
        color: Color,
        transform: TransformMatrix,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let corners = [
            transform.apply(Point::new(rect.x, rect.y)),
            transform.apply(Point::new(rect.right(), rect.y)),
            transform.apply(Point::new(rect.right(), rect.bottom())),
            transform.apply(Point::new(rect.x, rect.bottom())),
        ];
        let left = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let top = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let right = corners.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
        let bottom = corners.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        let mut min_x = left.floor().max(0.0) as u32;
        let mut min_y = top.floor().max(0.0) as u32;
        let mut max_x = right.ceil().min(self.width as f32) as u32;
        let mut max_y = bottom.ceil().min(self.height as f32) as u32;
        if let Some(clip) = scissor {
            min_x = min_x.max(clip.x.max(0.0).floor() as u32);
            min_y = min_y.max(clip.y.max(0.0).floor() as u32);
            max_x = max_x.min(clip.right().ceil().clamp(0.0, self.width as f32) as u32);
            max_y = max_y.min(clip.bottom().ceil().clamp(0.0, self.height as f32) as u32);
        }
        if min_x >= max_x || min_y >= max_y {
            return;
        }
        let color = color_to_array(color);
        let matrix = transform.to_array();
        let inv_x = if matrix[0].abs() > f32::EPSILON { 1.0 / matrix[0] } else { 0.0 };
        let inv_y = if matrix[3].abs() > f32::EPSILON { 1.0 / matrix[3] } else { 0.0 };
        // Elliptical corners only need four coverage levels to look smooth;
        // 4x4 would spend 4x the samples across the whole panel.
        const SAMPLE: u32 = 2;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let px0 = (x as f32 - matrix[4]) * inv_x;
                let px1 = (x as f32 + 1.0 - matrix[4]) * inv_x;
                let py0 = (y as f32 - matrix[5]) * inv_y;
                let py1 = (y as f32 + 1.0 - matrix[5]) * inv_y;
                let lx0 = px0.min(px1);
                let lx1 = px0.max(px1);
                let ly0 = py0.min(py1);
                let ly1 = py0.max(py1);
                let pixel_w = (lx1 - lx0).abs().max(0.0001);
                let pixel_h = (ly1 - ly0).abs().max(0.0001);
                let in_corner = (lx1 <= rect.x + radii[0] + pixel_w
                    && ly1 <= rect.y + radii[0] + pixel_h)
                    || (lx0 >= rect.right() - radii[1] - pixel_w
                        && ly1 <= rect.y + radii[1] + pixel_h)
                    || (lx0 >= rect.right() - radii[2] - pixel_w
                        && ly0 >= rect.bottom() - radii[2] - pixel_h)
                    || (lx1 <= rect.x + radii[3] + pixel_w
                        && ly0 >= rect.bottom() - radii[3] - pixel_h);
                if in_corner {
                    let mut covered = 0_u32;
                    for sy in 0..SAMPLE {
                        for sx in 0..SAMPLE {
                            let sx_local = lx0
                                + (sx as f32 + 0.5) / SAMPLE as f32 * pixel_w;
                            let sy_local = ly0
                                + (sy as f32 + 0.5) / SAMPLE as f32 * pixel_h;
                            if inside_rounded_rect(rect, radii, sx_local, sy_local) {
                                covered += 1;
                            }
                        }
                    }
                    if covered > 0 {
                        let mut blended = color;
                        blended[3] *= covered as f32 / 4.0;
                        self.blend(x, y, blended);
                    }
                } else {
                    let overlap_x =
                        (lx1.min(rect.right()) - lx0.max(rect.x)).max(0.0);
                    let overlap_y =
                        (ly1.min(rect.bottom()) - ly0.max(rect.y)).max(0.0);
                    let coverage = (overlap_x * overlap_y) / (pixel_w * pixel_h);
                    if coverage > 0.0 {
                        let mut blended = color;
                        blended[3] *= coverage.min(1.0);
                        self.blend(x, y, blended);
                    }
                }
            }
        }
    }

    fn fill_rounded_rect(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        rect: Rect,
        radius: Radius,
        color: Color,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let maximum = rect.width.min(rect.height) / 2.0;
        let tl = radius.top_left.clamp(0.0, maximum);
        let tr = radius.top_right.clamp(0.0, maximum);
        let br = radius.bottom_right.clamp(0.0, maximum);
        let bl = radius.bottom_left.clamp(0.0, maximum);
        let matrix = transform.to_array();
        if matrix[1].abs() < 0.0001 && matrix[2].abs() < 0.0001 {
            self.fill_axis_aligned_rounded_rect(
                scissor, rect, [tl, tr, br, bl], color, transform,
            );
            return;
        }
        let mut points = Vec::with_capacity(28);
        add_arc(
            &mut points,
            Point::new(rect.x + tl, rect.y + tl),
            tl,
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
        );
        add_arc(
            &mut points,
            Point::new(rect.right() - tr, rect.y + tr),
            tr,
            std::f32::consts::PI * 1.5,
            std::f32::consts::PI * 2.0,
        );
        add_arc(
            &mut points,
            Point::new(rect.right() - br, rect.bottom() - br),
            br,
            0.0,
            std::f32::consts::PI * 0.5,
        );
        add_arc(
            &mut points,
            Point::new(rect.x + bl, rect.bottom() - bl),
            bl,
            std::f32::consts::PI * 0.5,
            std::f32::consts::PI,
        );
        let center = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        let color = color_to_array(color);
        for index in 0..points.len() {
            self.fill_triangle(
                transform,
                scissor,
                center,
                points[index],
                points[(index + 1) % points.len()],
                [color; 3],
            );
        }
    }

    fn fill_circle(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        center: Point,
        radius: f32,
        color: Color,
    ) {
        if radius <= 0.0 {
            return;
        }
        let color = color_to_array(color);
        for segment in 0..64 {
            let first = segment as f32 * std::f32::consts::TAU / 64.0;
            let second = (segment + 1) as f32 * std::f32::consts::TAU / 64.0;
            self.fill_triangle(
                transform,
                scissor,
                center,
                Point::new(
                    center.x + radius * first.cos(),
                    center.y + radius * first.sin(),
                ),
                Point::new(
                    center.x + radius * second.cos(),
                    center.y + radius * second.sin(),
                ),
                [color; 3],
            );
        }
    }

    /// Draws the blur a rounded rectangle casts, offset from it.
    ///
    /// The mask is rasterized and blurred in device space. A rotated or
    /// skewed transform falls back to a solid rounded rectangle with the
    /// blur spread into its radius, since a rotated blur would need its own
    /// transformed mask.
    #[allow(clippy::too_many_arguments)]
    fn fill_shadow(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        rect: Rect,
        radius: f32,
        offset: Point,
        blur: f32,
        color: Color,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 || color.a == 0 {
            return;
        }
        let matrix = transform.to_array();
        let axis_aligned = matrix[1].abs() < 0.0001 && matrix[2].abs() < 0.0001;
        let shifted = Rect::new(
            rect.x + offset.x,
            rect.y + offset.y,
            rect.width,
            rect.height,
        );
        let blur = blur.max(0.0);
        if !axis_aligned {
            let spread = blur * 0.5;
            self.fill_rounded_rect(
                transform,
                scissor,
                Rect::new(
                    shifted.x - spread,
                    shifted.y - spread,
                    shifted.width + spread * 2.0,
                    shifted.height + spread * 2.0,
                ),
                Radius::all(radius + spread),
                color,
            );
            return;
        }
        let scale = transform.scale_factor().max(0.0001);
        let device = transform.transform_rect(shifted);
        let blur_px = blur * scale;
        let reach = blur_px * 2.5 + 2.0;
        let mut left = (device.x - reach).floor().max(0.0);
        let mut top = (device.y - reach).floor().max(0.0);
        let mut right = (device.right() + reach).ceil().min(self.width as f32);
        let mut bottom = (device.bottom() + reach).ceil().min(self.height as f32);
        if let Some(clip) = scissor {
            left = left.max(clip.x.max(0.0).floor());
            top = top.max(clip.y.max(0.0).floor());
            right = right.min(clip.right().ceil().clamp(0.0, self.width as f32));
            bottom = bottom.min(clip.bottom().ceil().clamp(0.0, self.height as f32));
        }
        if left >= right || top >= bottom {
            return;
        }
        let width = (right - left) as usize;
        let height = (bottom - top) as usize;
        let maximum = device.width.min(device.height) / 2.0;
        let corner = (radius * scale).clamp(0.0, maximum);
        let radii = [corner; 4];
        let mut mask = vec![0.0_f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut covered = 0.0;
                for sample_y in 0..2 {
                    for sample_x in 0..2 {
                        let px = left + x as f32 + (sample_x as f32 + 0.5) / 2.0;
                        let py = top + y as f32 + (sample_y as f32 + 0.5) / 2.0;
                        if inside_rounded_rect(device, radii, px, py) {
                            covered += 0.25;
                        }
                    }
                }
                mask[y * width + x] = covered;
            }
        }
        // Three box passes approximate the Gaussian the blur radius names.
        let pass = (blur_px * 0.5).round() as usize;
        if pass > 0 {
            for _ in 0..3 {
                box_blur(&mut mask, width, height, pass);
            }
        }
        let color = color_to_array(color);
        for y in 0..height {
            for x in 0..width {
                let coverage = mask[y * width + x];
                if coverage <= 0.001 {
                    continue;
                }
                self.blend(
                    left as u32 + x as u32,
                    top as u32 + y as u32,
                    [
                        color[0],
                        color[1],
                        color[2],
                        color[3] * coverage.min(1.0),
                    ],
                );
            }
        }
    }

    /// Draws a line in local coordinates, transforming the quad once so
    /// non-uniform scales stretch it the same way the GPU's local SVG mask
    /// is stretched by the full transform.
    fn draw_line_local(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        start: Point,
        end: Point,
        width: f32,
        color: Color,
    ) {
        let Some(points) = line_quad(start, end, width) else {
            return;
        };
        let points = [
            transform.apply(points[0]),
            transform.apply(points[1]),
            transform.apply(points[2]),
            transform.apply(points[3]),
        ];
        self.fill_line_quad(scissor, points, color);
    }

    /// Draws a line whose endpoints and thickness are already in device space,
    /// as [`DrawCommand::Line`] is rendered on the GPU.
    fn draw_line_device(
        &mut self,
        scissor: Option<Rect>,
        start: Point,
        end: Point,
        width: f32,
        color: Color,
    ) {
        let Some(points) = line_quad(start, end, width) else {
            return;
        };
        self.fill_line_quad(scissor, points, color);
    }

    fn fill_line_quad(&mut self, scissor: Option<Rect>, points: [Point; 4], color: Color) {
        let axis_aligned = ((points[0].x - points[1].x).abs() < 0.001
            && (points[1].y - points[2].y).abs() < 0.001
            && (points[2].x - points[3].x).abs() < 0.001
            && (points[3].y - points[0].y).abs() < 0.001)
            || ((points[0].y - points[1].y).abs() < 0.001
                && (points[1].x - points[2].x).abs() < 0.001
                && (points[2].y - points[3].y).abs() < 0.001
                && (points[3].x - points[0].x).abs() < 0.001);
        if axis_aligned {
            let left = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
            let top = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let right = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
            let bottom = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
            self.fill_axis_aligned_rect(
                scissor,
                Rect::new(left, top, right - left, bottom - top),
                color,
            );
            return;
        }
        let color = color_to_array(color);
        self.fill_triangle(
            TransformMatrix::IDENTITY,
            scissor,
            points[0],
            points[1],
            points[2],
            [color; 3],
        );
        self.fill_triangle(
            TransformMatrix::IDENTITY,
            scissor,
            points[0],
            points[2],
            points[3],
            [color; 3],
        );
    }

    fn fill_gradient(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        rect: Rect,
        radius: Radius,
        start: Point,
        end: Point,
        stops: &[GradientStop],
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 || stops.is_empty() {
            return;
        }
        let mut ordered = stops.to_vec();
        ordered.sort_by(|left, right| left.offset.total_cmp(&right.offset));
        let maximum = rect.width.min(rect.height) / 2.0;
        let radii = [
            radius.top_left.clamp(0.0, maximum),
            radius.top_right.clamp(0.0, maximum),
            radius.bottom_right.clamp(0.0, maximum),
            radius.bottom_left.clamp(0.0, maximum),
        ];
        let rounded = radii.iter().any(|value| *value > 0.0);
        let corners = [
            (Point::new(rect.x, rect.y), Point::new(0.0, 0.0)),
            (Point::new(rect.right(), rect.y), Point::new(1.0, 0.0)),
            (
                Point::new(rect.right(), rect.bottom()),
                Point::new(1.0, 1.0),
            ),
            (Point::new(rect.x, rect.bottom()), Point::new(0.0, 1.0)),
        ];
        let colors = corners.map(|(_, normalized)| {
            color_to_array(sample_gradient(
                &ordered,
                gradient_offset(start, end, normalized),
            ))
        });
        let matrix = transform.to_array();
        if matrix[1].abs() < 0.0001 && matrix[2].abs() < 0.0001 {
            let left = corners[0].0.x.min(corners[2].0.x);
            let top = corners[0].0.y.min(corners[2].0.y);
            let right = corners[0].0.x.max(corners[2].0.x);
            let bottom = corners[0].0.y.max(corners[2].0.y);
            let width = (right - left).max(1.0);
            let height = (bottom - top).max(1.0);
            let mut min_x = left.floor().max(0.0) as u32;
            let mut min_y = top.floor().max(0.0) as u32;
            let mut max_x = right.ceil().min(self.width as f32) as u32;
            let mut max_y = bottom.ceil().min(self.height as f32) as u32;
            if let Some(clip) = scissor {
                min_x = min_x.max(clip.x.max(0.0).floor() as u32);
                min_y = min_y.max(clip.y.max(0.0).floor() as u32);
                max_x = max_x.min(clip.right().ceil().clamp(0.0, self.width as f32) as u32);
                max_y = max_y.min(clip.bottom().ceil().clamp(0.0, self.height as f32) as u32);
            }
            for y in min_y..max_y {
                let v = (y as f32 + 0.5 - top) / height;
                for x in min_x..max_x {
                    let u = (x as f32 + 0.5 - left) / width;
                    let top_color = [
                        colors[0][0] + (colors[1][0] - colors[0][0]) * u,
                        colors[0][1] + (colors[1][1] - colors[0][1]) * u,
                        colors[0][2] + (colors[1][2] - colors[0][2]) * u,
                        colors[0][3] + (colors[1][3] - colors[0][3]) * u,
                    ];
                    let bottom_color = [
                        colors[3][0] + (colors[2][0] - colors[3][0]) * u,
                        colors[3][1] + (colors[2][1] - colors[3][1]) * u,
                        colors[3][2] + (colors[2][2] - colors[3][2]) * u,
                        colors[3][3] + (colors[2][3] - colors[3][3]) * u,
                    ];
                    let mut color = [
                        top_color[0] + (bottom_color[0] - top_color[0]) * v,
                        top_color[1] + (bottom_color[1] - top_color[1]) * v,
                        top_color[2] + (bottom_color[2] - top_color[2]) * v,
                        top_color[3] + (bottom_color[3] - top_color[3]) * v,
                    ];
                    if rounded {
                        let cx = x as f32 + 0.5;
                        let cy = y as f32 + 0.5;
                        let in_corner = (cx < rect.x + radii[0] && cy < rect.y + radii[0])
                            || (cx > rect.right() - radii[1] && cy < rect.y + radii[1])
                            || (cx > rect.right() - radii[2] && cy > rect.bottom() - radii[2])
                            || (cx < rect.x + radii[3] && cy > rect.bottom() - radii[3]);
                        if in_corner {
                            let mut covered = 0_u32;
                            for sy in 0..2 {
                                for sx in 0..2 {
                                    let sample_x = x as f32 + (sx as f32 + 0.5) / 2.0;
                                    let sample_y = y as f32 + (sy as f32 + 0.5) / 2.0;
                                    if inside_rounded_rect(rect, radii, sample_x, sample_y) {
                                        covered += 1;
                                    }
                                }
                            }
                            if covered == 0 {
                                continue;
                            }
                            color[3] *= covered as f32 / 4.0;
                        }
                    }
                    self.blend(x, y, color);
                }
            }
            return;
        }
        self.fill_triangle(
            transform,
            scissor,
            corners[0].0,
            corners[1].0,
            corners[2].0,
            [colors[0], colors[1], colors[2]],
        );
        self.fill_triangle(
            transform,
            scissor,
            corners[0].0,
            corners[2].0,
            corners[3].0,
            [colors[0], colors[2], colors[3]],
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_image(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        rect: Rect,
        source: &ImageHandle,
        source_rect: ImageSourceRect,
        radius: Radius,
        grayscale: bool,
    ) {
        let Some(data) = source.data() else {
            self.unsupported_images += 1;
            return;
        };
        if rect.width <= 0.0
            || rect.height <= 0.0
            || source_rect.width <= 0.0
            || source_rect.height <= 0.0
        {
            return;
        }
        let Some(inverse) = transform.inverse() else {
            return;
        };
        let corners = [
            transform.apply(Point::new(rect.x, rect.y)),
            transform.apply(Point::new(rect.right(), rect.y)),
            transform.apply(Point::new(rect.right(), rect.bottom())),
            transform.apply(Point::new(rect.x, rect.bottom())),
        ];
        let left = corners
            .iter()
            .map(|point| point.x)
            .fold(f32::INFINITY, f32::min);
        let top = corners
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);
        let right = corners
            .iter()
            .map(|point| point.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = corners
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let mut min_x = left.floor().max(0.0) as u32;
        let mut min_y = top.floor().max(0.0) as u32;
        let mut max_x = right.ceil().min(self.width as f32).max(0.0) as u32;
        let mut max_y = bottom.ceil().min(self.height as f32).max(0.0) as u32;
        if let Some(clip) = scissor {
            min_x = min_x.max(clip.x.max(0.0).floor() as u32);
            min_y = min_y.max(clip.y.max(0.0).floor() as u32);
            max_x = max_x.min(clip.right().ceil().clamp(0.0, self.width as f32) as u32);
            max_y = max_y.min(clip.bottom().ceil().clamp(0.0, self.height as f32) as u32);
        }
        if min_x >= max_x || min_y >= max_y {
            return;
        }
        let maximum = rect.width.min(rect.height) / 2.0;
        let radii = [
            radius.top_left.clamp(0.0, maximum),
            radius.top_right.clamp(0.0, maximum),
            radius.bottom_right.clamp(0.0, maximum),
            radius.bottom_left.clamp(0.0, maximum),
        ];
        const SAMPLE: u32 = 4;
        const SAMPLE_COUNT: u32 = SAMPLE * SAMPLE;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let mut covered = 0_u32;
                for sy in 0..SAMPLE {
                    for sx in 0..SAMPLE {
                        let device = Point::new(
                            x as f32 + (sx as f32 + 0.5) / SAMPLE as f32,
                            y as f32 + (sy as f32 + 0.5) / SAMPLE as f32,
                        );
                        let local = inverse.apply(device);
                        if inside_rounded_rect(rect, radii, local.x, local.y) {
                            covered += 1;
                        }
                    }
                }
                if covered == 0 {
                    continue;
                }
                let local = inverse.apply(Point::new(x as f32 + 0.5, y as f32 + 0.5));
                let u = source_rect.x
                    + (local.x - rect.x) / rect.width * source_rect.width;
                let v = source_rect.y
                    + (local.y - rect.y) / rect.height * source_rect.height;
                let mut color = sample_rgba_bilinear(
                    data.pixels(),
                    data.width(),
                    data.height(),
                    u,
                    v,
                );
                if grayscale {
                    let luma = (0.2126 * color[0]
                        + 0.7152 * color[1]
                        + 0.0722 * color[2])
                        .round();
                    color[0] = luma;
                    color[1] = luma;
                    color[2] = luma;
                }
                color[3] *= covered as f32 / SAMPLE_COUNT as f32;
                self.blend(x, y, color);
            }
        }
    }

    fn measure_text(&self, text: &str, font_size: f32) -> f32 {
        let font_size = font_size.max(1.0);
        match self.font.as_ref() {
            Some(font) => text
                .chars()
                .map(|character| font.glyph_advance(character, font_size))
                .sum(),
            None => text.chars().count() as f32 * font_size * 0.6,
        }
    }

    fn line_height(&self, font_size: f32) -> f32 {
        match self.font.as_ref() {
            Some(font) => font.line_height(font_size),
            None => font_size * 1.25,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_layout(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        origin: Point,
        layout: &TextLayout,
        weight: u16,
        italic: bool,
        color: Color,
    ) {
        let color = color_to_array(color);
        let bold_offset = if weight >= 600 {
            (layout.font_size * 0.035).clamp(0.35, 1.0)
        } else {
            0.0
        };
        for (line_index, line) in layout.lines.iter().enumerate() {
            let baseline_y = origin.y + layout.baseline(line_index);
            let line_transform = if italic {
                transform.then(TransformMatrix::italic_shear(baseline_y, 0.2))
            } else {
                transform
            };
            let mut pen_x = origin.x + line.x_offset;
            for run in &line.runs {
                for glyph in &run.glyphs {
                    let stack_id = self.stack.as_ref().map_or(0, |stack| stack.id());
                    let bitmap = cached_glyph(
                        stack_id,
                        &run.font,
                        glyph.glyph_id,
                        layout.font_size,
                        weight >= 600,
                        italic,
                    );
                    let x = pen_x + glyph.offset_x;
                    let y = baseline_y - glyph.offset_y;
                    self.fill_glyph_bitmap(
                        line_transform,
                        scissor,
                        &bitmap,
                        x,
                        y,
                        0.0,
                        color,
                    );
                    if bold_offset > 0.0 {
                        self.fill_glyph_bitmap(
                            line_transform,
                            scissor,
                            &bitmap,
                            x,
                            y,
                            bold_offset,
                            color,
                        );
                    }
                    pen_x += glyph.advance;
                }
            }
        }
    }

    fn draw_text(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        origin: Point,
        text: &str,
        font_size: f32,
        weight: u16,
        italic: bool,
        _mono: bool,
        _family: Option<&str>,
        color: Color,
    ) {        let Some(font) = self.font.clone() else {
            self.draw_bitmap_text(transform, scissor, origin, text, font_size, color);
            return;
        };
        let font_size = font_size.max(1.0);
        let line_height = font.line_height(font_size);
        let baseline_offset = font.ascent(font_size);
        let color = color_to_array(color);
        // Single-face rasterization has no bold face; thicken strokes by
        // stamping the coverage bitmap a fraction of the em to the right.
        let bold_offset = if weight >= 600 {
            (font_size * 0.035).clamp(0.35, 1.0)
        } else {
            0.0
        };
        for (line_index, line) in text.split('\n').enumerate() {
            let mut cursor_x = origin.x;
            let baseline_y = origin.y + line_index as f32 * line_height + baseline_offset;
            let line_transform = if italic {
                transform.then(TransformMatrix::italic_shear(baseline_y, 0.2))
            } else {
                transform
            };
            for character in line.chars() {
                if !font.has_glyph(character) {
                    self.unsupported_glyphs += 1;
                    cursor_x += font_size * 0.5;
                    continue;
                }
                let stack_id = self.stack.as_ref().map_or(0, |stack| stack.id());
                let glyph_id = font.glyph_id(character);
                let bitmap = cached_glyph(
                    stack_id,
                    &font,
                    glyph_id,
                    font_size,
                    weight >= 600,
                    italic,
                );
                self.fill_glyph_bitmap(
                    line_transform,
                    scissor,
                    &bitmap,
                    cursor_x,
                    baseline_y,
                    0.0,
                    color,
                );
                if bold_offset > 0.0 {
                    self.fill_glyph_bitmap(
                        line_transform,
                        scissor,
                        &bitmap,
                        cursor_x,
                        baseline_y,
                        bold_offset,
                        color,
                    );
                }
                cursor_x += font.glyph_advance(character, font_size);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_glyph_bitmap(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        bitmap: &GlyphBitmap,
        pen_x: f32,
        baseline_y: f32,
        offset: f32,
        color: [f32; 4],
    ) {
        for row in 0..bitmap.height {
            for column in 0..bitmap.width {
                let coverage =
                    bitmap.coverage[(row * bitmap.width + column) as usize] as f32 / 255.0;
                if coverage <= 0.0 {
                    continue;
                }
                let x = pen_x + bitmap.bearing_x as f32 + column as f32 + offset;
                let y = baseline_y + bitmap.bearing_y as f32 + row as f32;
                let points = [
                    Point::new(x, y),
                    Point::new(x + 1.0, y),
                    Point::new(x + 1.0, y + 1.0),
                    Point::new(x, y + 1.0),
                ];
                let mut glyph_color = color;
                glyph_color[3] *= coverage;
                self.fill_triangle(
                    transform,
                    scissor,
                    points[0],
                    points[1],
                    points[2],
                    [glyph_color; 3],
                );
                self.fill_triangle(
                    transform,
                    scissor,
                    points[0],
                    points[2],
                    points[3],
                    [glyph_color; 3],
                );
            }
        }
    }

    fn draw_bitmap_text(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        origin: Point,
        text: &str,
        font_size: f32,
        color: Color,
    ) {
        let font_size = font_size.max(1.0);
        let glyph_scale = font_size / 8.0;
        let line_height = font_size * 1.25;
        let color = color_to_array(color);
        for (line_index, line) in text.split('\n').enumerate() {
            let mut cursor_x = origin.x;
            let cursor_y = origin.y + line_index as f32 * line_height;
            for character in line.chars() {
                let Some(glyph) = BASIC_FONTS.get(character) else {
                    self.unsupported_glyphs += 1;
                    cursor_x += font_size * 0.6;
                    continue;
                };
                for (row, bits) in glyph.iter().enumerate() {
                    for column in 0..8 {
                        if bits & (1 << column) == 0 {
                            continue;
                        }
                        let x = cursor_x + column as f32 * glyph_scale;
                        let y = cursor_y + row as f32 * glyph_scale;
                        let points = [
                            Point::new(x, y),
                            Point::new(x + glyph_scale, y),
                            Point::new(x + glyph_scale, y + glyph_scale),
                            Point::new(x, y + glyph_scale),
                        ];
                        self.fill_triangle(
                            transform, scissor, points[0], points[1], points[2], [color; 3],
                        );
                        self.fill_triangle(
                            transform, scissor, points[0], points[2], points[3], [color; 3],
                        );
                    }
                }
                cursor_x += font_size * 0.62;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_triangle(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        a: Point,
        b: Point,
        c: Point,
        colors: [[f32; 4]; 3],
    ) {
        let a = transform.apply(a);
        let b = transform.apply(b);
        let c = transform.apply(c);
        let area = edge(a, b, c);
        if area.abs() <= f32::EPSILON {
            return;
        }
        let sign = if area > 0.0 { 1.0 } else { -1.0 };
        let mut min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as u32;
        let mut min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as u32;
        let mut max_x = a.x.max(b.x).max(c.x).ceil().min(self.width as f32) as u32;
        let mut max_y = a.y.max(b.y).max(c.y).ceil().min(self.height as f32) as u32;
        if let Some(clip) = scissor {
            min_x = min_x.max(clip.x.max(0.0).floor() as u32);
            min_y = min_y.max(clip.y.max(0.0).floor() as u32);
            max_x = max_x.min(clip.right().ceil().clamp(0.0, self.width as f32) as u32);
            max_y = max_y.min(clip.bottom().ceil().clamp(0.0, self.height as f32) as u32);
        }
        if min_x >= max_x || min_y >= max_y {
            return;
        }

        let inverse_area = 1.0 / area.abs();
        // Half-pixel extents of the edge function gradients. A pixel is fully
        // covered when all three signed edge values exceed these margins; only
        // boundary pixels need the 4x4 coverage samples.
        let margins = [
            (b.x - c.x).abs() + (b.y - c.y).abs(),
            (c.x - a.x).abs() + (c.y - a.y).abs(),
            (a.x - b.x).abs() + (a.y - b.y).abs(),
        ];
        const SAMPLE: u32 = 4;
        const SAMPLE_COUNT: u32 = SAMPLE * SAMPLE;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let point = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                let weights = [
                    edge(b, c, point) * sign,
                    edge(c, a, point) * sign,
                    edge(a, b, point) * sign,
                ];
                let normalized = [
                    weights[0] * inverse_area,
                    weights[1] * inverse_area,
                    weights[2] * inverse_area,
                ];
                let fully_covered = weights[0] >= margins[0] * 0.5
                    && weights[1] >= margins[1] * 0.5
                    && weights[2] >= margins[2] * 0.5;
                if fully_covered {
                    let color = [
                        colors[0][0] * normalized[0]
                            + colors[1][0] * normalized[1]
                            + colors[2][0] * normalized[2],
                        colors[0][1] * normalized[0]
                            + colors[1][1] * normalized[1]
                            + colors[2][1] * normalized[2],
                        colors[0][2] * normalized[0]
                            + colors[1][2] * normalized[1]
                            + colors[2][2] * normalized[2],
                        colors[0][3] * normalized[0]
                            + colors[1][3] * normalized[1]
                            + colors[2][3] * normalized[2],
                    ];
                    self.blend(x, y, color);
                    continue;
                }

                let mut covered = 0_u32;
                let mut accumulated = [0.0_f32; 4];
                for sy in 0..SAMPLE {
                    for sx in 0..SAMPLE {
                        let sample = Point::new(
                            x as f32 + (sx as f32 + 0.5) / SAMPLE as f32,
                            y as f32 + (sy as f32 + 0.5) / SAMPLE as f32,
                        );
                        let sample_weights = [
                            edge(b, c, sample) * sign,
                            edge(c, a, sample) * sign,
                            edge(a, b, sample) * sign,
                        ];
                        if sample_weights.iter().all(|weight| *weight >= -0.0001) {
                            covered += 1;
                            let sample_norm = [
                                sample_weights[0] * inverse_area,
                                sample_weights[1] * inverse_area,
                                sample_weights[2] * inverse_area,
                            ];
                            for channel in 0..3 {
                                accumulated[channel] += colors[0][channel] * sample_norm[0]
                                    + colors[1][channel] * sample_norm[1]
                                    + colors[2][channel] * sample_norm[2];
                            }
                            accumulated[3] += colors[0][3] * sample_norm[0]
                                + colors[1][3] * sample_norm[1]
                                + colors[2][3] * sample_norm[2];
                        }
                    }
                }
                if covered > 0 {
                    let coverage = covered as f32 / SAMPLE_COUNT as f32;
                    let divisor = covered as f32;
                    self.blend(
                        x,
                        y,
                        [
                            accumulated[0] / divisor,
                            accumulated[1] / divisor,
                            accumulated[2] / divisor,
                            (accumulated[3] / divisor) * coverage,
                        ],
                    );
                }
            }
        }
    }
    fn fill_contours(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        contours: &[SvgContour],
        color: Color,
    ) {
        let transformed = contours
            .iter()
            .map(|contour| {
                contour
                    .points
                    .iter()
                    .map(|point| transform.apply(*point))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let points = transformed.iter().flatten().collect::<Vec<_>>();
        if points.is_empty() {
            return;
        }
        let color = color_to_array(color);
        let mut min_x = points
            .iter()
            .map(|point| point.x)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as u32;
        let mut min_y = points
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as u32;
        let mut max_x = points
            .iter()
            .map(|point| point.x)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(self.width as f32) as u32;
        let mut max_y = points
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(self.height as f32) as u32;
        if let Some(clip) = scissor {
            min_x = min_x.max(clip.x.max(0.0).floor() as u32);
            min_y = min_y.max(clip.y.max(0.0).floor() as u32);
            max_x = max_x.min(clip.right().ceil().clamp(0.0, self.width as f32) as u32);
            max_y = max_y.min(clip.bottom().ceil().clamp(0.0, self.height as f32) as u32);
        }
        if min_x >= max_x || min_y >= max_y {
            return;
        }
        for y in min_y..max_y {
            for x in min_x..max_x {
                let point = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                let mut inside = false;
                for contour in &transformed {
                    let count = contour.len();
                    if count < 3 {
                        continue;
                    }
                    for index in 0..count {
                        let first = contour[index];
                        let second = contour[(index + 1) % count];
                        if (first.y > point.y) != (second.y > point.y) {
                            let crossing = (second.x - first.x) * (point.y - first.y)
                                / (second.y - first.y)
                                + first.x;
                            if point.x < crossing {
                                inside = !inside;
                            }
                        }
                    }
                }
                if inside {
                    self.blend(x, y, color);
                }
            }
        }
    }

    fn stroke_contours(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        contours: &[SvgContour],
        stroke_width: f32,
        color: Color,
    ) {
        if stroke_width <= 0.0 {
            return;
        }
        for contour in contours {
            if contour.points.len() < 2 {
                continue;
            }
            for pair in contour.points.windows(2) {
                self.draw_line_local(transform, scissor, pair[0], pair[1], stroke_width, color);
            }
            if contour.closed {
                let first = contour.points[0];
                let last = *contour.points.last().expect("contour has points");
                self.draw_line_local(transform, scissor, last, first, stroke_width, color);
            }
        }
    }

    fn blend(&mut self, x: u32, y: u32, color: [f32; 4]) {
        let alpha = color[3].clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let index = y as usize * self.width as usize + x as usize;
        let destination = self.pixels[index];
        let destination_r = ((destination >> 16) & 0xff) as f32;
        let destination_g = ((destination >> 8) & 0xff) as f32;
        let destination_b = (destination & 0xff) as f32;
        let source_r = color[0].clamp(0.0, 1.0) * 255.0;
        let source_g = color[1].clamp(0.0, 1.0) * 255.0;
        let source_b = color[2].clamp(0.0, 1.0) * 255.0;
        let red = (source_r * alpha + destination_r * (1.0 - alpha)).round() as u32;
        let green = (source_g * alpha + destination_g * (1.0 - alpha)).round() as u32;
        let blue = (source_b * alpha + destination_b * (1.0 - alpha)).round() as u32;
        self.pixels[index] = (red.min(255) << 16) | (green.min(255) << 8) | blue.min(255);
    }
}

/// The four corners of a line quad in whatever space its endpoints are in,
/// or `None` when the line has no width or length.
fn line_quad(start: Point, end: Point, width: f32) -> Option<[Point; 4]> {
    if width <= 0.0 {
        return None;
    }
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f32::EPSILON {
        return None;
    }
    let half = width / 2.0;
    let normal = Point::new(-dy / length * half, dx / length * half);
    Some([
        Point::new(start.x + normal.x, start.y + normal.y),
        Point::new(end.x + normal.x, end.y + normal.y),
        Point::new(end.x - normal.x, end.y - normal.y),
        Point::new(start.x - normal.x, start.y - normal.y),
    ])
}

/// Applies one box-blur pass horizontally and one vertically to a row-major
/// coverage mask, with clamped edges.
pub(crate) fn box_blur(mask: &mut [f32], width: usize, height: usize, radius: usize) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }
    let norm = 1.0 / (radius * 2 + 1) as f32;
    let mut horizontal = vec![0.0_f32; mask.len()];
    for y in 0..height {
        let row = y * width;
        let mut sum = 0.0;
        for k in 0..=radius {
            sum += mask[row + k.min(width - 1)];
        }
        for x in 0..width {
            horizontal[row + x] = sum * norm;
            if x + 1 < width {
                let entering = (x + 1 + radius).min(width - 1);
                let leaving = (x as isize - radius as isize).max(0) as usize;
                sum += mask[row + entering] - mask[row + leaving];
            }
        }
    }
    let mut vertical = vec![0.0_f32; horizontal.len()];
    for x in 0..width {
        let mut sum = 0.0;
        for k in 0..=radius {
            sum += horizontal[k.min(height - 1) * width + x];
        }
        for y in 0..height {
            vertical[y * width + x] = sum * norm;
            if y + 1 < height {
                let entering = (y + 1 + radius).min(height - 1);
                let leaving = (y as isize - radius as isize).max(0) as usize;
                sum += horizontal[entering * width + x] - horizontal[leaving * width + x];
            }
        }
    }
    mask.copy_from_slice(&vertical);
}

fn add_arc(points: &mut Vec<Point>, center: Point, radius: f32, start: f32, end: f32) {
    // Use enough segments that a large radius does not become a visible
    // polyline; small controls keep the original six-segment floor.
    let span = (end - start).abs();
    let segments = ((radius * span / 4.0).ceil() as usize).clamp(6, 128);
    for segment in 0..=segments {
        let angle = start + (end - start) * segment as f32 / segments as f32;
        points.push(Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
}


pub(crate) fn inside_rounded_rect(rect: Rect, radii: [f32; 4], x: f32, y: f32) -> bool {
    if x < rect.x || x > rect.right() || y < rect.y || y > rect.bottom() {
        return false;
    }
    let (tl, tr, br, bl) = (radii[0], radii[1], radii[2], radii[3]);
    if x < rect.x + tl && y < rect.y + tl {
        let dx = x - (rect.x + tl);
        let dy = y - (rect.y + tl);
        dx * dx + dy * dy <= tl * tl
    } else if x > rect.right() - tr && y < rect.y + tr {
        let dx = x - (rect.right() - tr);
        let dy = y - (rect.y + tr);
        dx * dx + dy * dy <= tr * tr
    } else if x > rect.right() - br && y > rect.bottom() - br {
        let dx = x - (rect.right() - br);
        let dy = y - (rect.bottom() - br);
        dx * dx + dy * dy <= br * br
    } else if x < rect.x + bl && y > rect.bottom() - bl {
        let dx = x - (rect.x + bl);
        let dy = y - (rect.bottom() - bl);
        dx * dx + dy * dy <= bl * bl
    } else {
        true
    }
}

fn edge(a: Point, b: Point, c: Point) -> f32 {
    (c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)
}

fn sample_gradient(stops: &[GradientStop], offset: f32) -> Color {
    let offset = offset.clamp(0.0, 1.0);
    let first = stops[0];
    if offset <= first.offset {
        return first.color;
    }
    for pair in stops.windows(2) {
        let left = pair[0];
        let right = pair[1];
        if offset <= right.offset {
            let span = (right.offset - left.offset).max(f32::EPSILON);
            let amount = (offset - left.offset) / span;
            return interpolate_color(left.color, right.color, amount);
        }
    }
    stops[stops.len() - 1].color
}

fn interpolate_color(left: Color, right: Color, amount: f32) -> Color {
    let mix = |a: u8, b: u8| {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::rgba(
        mix(left.r, right.r),
        mix(left.g, right.g),
        mix(left.b, right.b),
        mix(left.a, right.a),
    )
}

fn gradient_offset(start: Point, end: Point, point: Point) -> f32 {
    let direction = Point::new(end.x - start.x, end.y - start.y);
    let length_squared = direction.x * direction.x + direction.y * direction.y;
    if length_squared <= f32::EPSILON {
        return 0.0;
    }
    ((point.x - start.x) * direction.x + (point.y - start.y) * direction.y) / length_squared
}

fn sample_rgba_bilinear(
    pixels: &[u8],
    width: u32,
    height: u32,
    u: f32,
    v: f32,
) -> [f32; 4] {
    if width == 0 || height == 0 {
        return [0.0; 4];
    }
    let sample_x = (u.clamp(0.0, 1.0) * width as f32 - 0.5).clamp(0.0, width as f32 - 1.0);
    let sample_y = (v.clamp(0.0, 1.0) * height as f32 - 0.5).clamp(0.0, height as f32 - 1.0);
    let x0 = sample_x.floor() as usize;
    let y0 = sample_y.floor() as usize;
    let x1 = (x0 + 1).min(width as usize - 1);
    let y1 = (y0 + 1).min(height as usize - 1);
    let fx = sample_x - x0 as f32;
    let fy = sample_y - y0 as f32;
    let texel = |x: usize, y: usize, channel: usize| {
        f32::from(pixels[(y * width as usize + x) * 4 + channel]) / 255.0
    };
    let mut color = [0.0; 4];
    for (channel, value) in color.iter_mut().enumerate() {
        let top = texel(x0, y0, channel) * (1.0 - fx) + texel(x1, y0, channel) * fx;
        let bottom =
            texel(x0, y1, channel) * (1.0 - fx) + texel(x1, y1, channel) * fx;
        *value = top * (1.0 - fy) + bottom * fy;
    }
    color
}

fn pack_color(color: Color) -> u32 {
    (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
}

fn clear_pixels(pixels: &mut [u32], width: u32, rect: Rect, background: Color) {
    let packed = pack_color(background);
    let min_x = rect.x.max(0.0).floor() as u32;
    let min_y = rect.y.max(0.0).floor() as u32;
    let max_x = rect.right().ceil().clamp(0.0, width as f32) as u32;
    let height = pixels.len() as u32 / width.max(1);
    let max_y = rect.bottom().ceil().clamp(0.0, height as f32) as u32;
    for y in min_y..max_y {
        let row = y as usize * width as usize;
        for pixel in &mut pixels[row + min_x as usize..row + max_x as usize] {
            *pixel = packed;
        }
    }
}

fn prefilter_display(display: &DisplayList, damage: Rect) -> DisplayList {
    let mut commands = Vec::new();
    let mut transform = TransformMatrix::IDENTITY;
    let mut transform_stack = Vec::new();
    let mut clip: Option<Rect> = None;
    let mut clip_stack = Vec::new();
    for command in &display.commands {
        match command {
            DrawCommand::TransformPush { transform: local, .. } => {
                transform_stack.push(transform);
                transform = transform.then(TransformMatrix::from_transform(*local));
                commands.push(command.clone());
            }
            DrawCommand::TransformPop => {
                if let Some(previous) = transform_stack.pop() {
                    transform = previous;
                }
                commands.push(command.clone());
            }
            DrawCommand::ScissorPush { rect } => {
                clip_stack.push(clip);
                let transformed = transform.transform_rect(*rect);
                clip = Some(match clip {
                    Some(current) => intersect_rects(current, transformed),
                    None => transformed,
                });
                commands.push(command.clone());
            }
            DrawCommand::ScissorPop => {
                if let Some(previous) = clip_stack.pop() {
                    clip = previous;
                }
                commands.push(command.clone());
            }
            _ => {
                let visible = command_device_bounds(command, transform)
                    .is_none_or(|bounds| {
                        let bounds = clip.map_or(bounds, |clip| intersect_rects(bounds, clip));
                        let hit = intersect_rects(bounds, damage);
                        hit.width > 0.0 && hit.height > 0.0
                    });
                if visible {
                    commands.push(command.clone());
                }
            }
        }
    }
    DisplayList::from_commands(commands)
}

fn add_counts(total: &mut CommandCounts, counts: CommandCounts) {
    total.rect += counts.rect;
    total.rounded_rect += counts.rounded_rect;
    total.circle += counts.circle;
    total.line += counts.line;
    total.text += counts.text;
    total.svg_path += counts.svg_path;
    total.linear_gradient += counts.linear_gradient;
    total.image += counts.image;
    total.scissor_push += counts.scissor_push;
    total.scissor_pop += counts.scissor_pop;
    total.transform_push += counts.transform_push;
    total.transform_pop += counts.transform_pop;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::FontRegistry;
    use crate::icon::Icon;
    use crate::image::ImageData;
    use crate::paint::Transform;
    use std::sync::Mutex;

    static CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn pixel(frame: &CpuFrame, x: u32, y: u32) -> u32 {
        frame.pixels[y as usize * frame.width as usize + x as usize]
    }

    #[test]
    fn rasterizes_rectangles_to_expected_pixels() {
        let display = DisplayList::from_commands(vec![DrawCommand::Rect {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 10.0,
            color: Color::rgb(255, 0, 0),
        }]);
        let frame =
            CpuRasterizer::rasterize(&display, RenderSize::new(64, 64), Color::BLACK).unwrap();
        assert_eq!(pixel(&frame, 15, 15), 0x00ff0000);
        assert_eq!(pixel(&frame, 5, 5), 0x00000000);
        assert_eq!(frame.command_counts.rect, 1);
    }

    #[test]
    fn scissor_clips_rectangle_pixels() {
        let display = DisplayList::from_commands(vec![
            DrawCommand::ScissorPush {
                rect: Rect::new(4.0, 4.0, 8.0, 8.0),
            },
            DrawCommand::Rect {
                x: 0.0,
                y: 0.0,
                width: 64.0,
                height: 64.0,
                color: Color::rgb(0, 255, 0),
            },
            DrawCommand::ScissorPop,
        ]);
        let frame =
            CpuRasterizer::rasterize(&display, RenderSize::new(64, 64), Color::BLACK).unwrap();
        assert_eq!(pixel(&frame, 5, 5), 0x0000ff00);
        assert_eq!(pixel(&frame, 20, 20), 0x00000000);
    }

    #[test]
    fn transform_offsets_rectangle_pixels() {
        let display = DisplayList::from_commands(vec![
            DrawCommand::TransformPush {
                transform: Transform::translate(Point::new(10.0, 0.0)),
            },
            DrawCommand::Rect {
                x: 0.0,
                y: 0.0,
                width: 8.0,
                height: 8.0,
                color: Color::rgb(0, 0, 255),
            },
            DrawCommand::TransformPop,
        ]);
        let frame =
            CpuRasterizer::rasterize(&display, RenderSize::new(32, 32), Color::BLACK).unwrap();
        assert_eq!(pixel(&frame, 12, 4), 0x000000ff);
        assert_eq!(pixel(&frame, 2, 4), 0x00000000);
    }

    #[test]
    fn image_samples_quadrants_and_alpha() {
        let pixels = [
            255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let source =
            ImageHandle::from_data("quadrants", ImageData::new(&pixels, 2, 2)).unwrap();
        let display = DisplayList::from_commands(vec![DrawCommand::Image {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
            source,
            source_rect: ImageSourceRect::default(),
            radius: Radius::default(),
            grayscale: false,
        }]);
        let frame = CpuRasterizer::rasterize_with_font(
            &display,
            RenderSize::new(64, 64),
            Color::TRANSPARENT,
            None,
        )
        .unwrap();
        assert_eq!(pixel(&frame, 8, 8), 0x00ff0000);
        assert_eq!(pixel(&frame, 56, 8), 0x0000ff00);
        assert_eq!(pixel(&frame, 8, 56), 0x000000ff);
        assert_eq!(pixel(&frame, 56, 56), 0x00ffffff);
        assert_eq!(frame.unsupported_commands, 0);

        let translucent = [255_u8, 0, 0, 128];
        let source =
            ImageHandle::from_data("alpha", ImageData::new(&translucent, 1, 1)).unwrap();
        let display = DisplayList::from_commands(vec![DrawCommand::Image {
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 8.0,
            source,
            source_rect: ImageSourceRect::default(),
            radius: Radius::default(),
            grayscale: false,
        }]);
        let frame = CpuRasterizer::rasterize_with_font(
            &display,
            RenderSize::new(8, 8),
            Color::BLACK,
            None,
        )
        .unwrap();
        let red = (pixel(&frame, 4, 4) >> 16) & 0xff;
        assert!((125..=130).contains(&red), "alpha blend produced red={red}");
    }

    #[test]
    fn image_respects_scissor_and_transform() {
        let pixels = [255_u8, 0, 0, 255];
        let source = ImageHandle::from_data("scissored", ImageData::new(&pixels, 1, 1)).unwrap();
        let display = DisplayList::from_commands(vec![
            DrawCommand::ScissorPush {
                rect: Rect::new(10.0, 10.0, 12.0, 12.0),
            },
            DrawCommand::TransformPush {
                transform: Transform::translate(Point::new(10.0, 10.0)),
            },
            DrawCommand::Image {
                x: 0.0,
                y: 0.0,
                width: 16.0,
                height: 16.0,
                source,
                source_rect: ImageSourceRect::default(),
                radius: Radius::default(),
                grayscale: false,
            },
            DrawCommand::TransformPop,
            DrawCommand::ScissorPop,
        ]);
        let frame = CpuRasterizer::rasterize_with_font(
            &display,
            RenderSize::new(32, 32),
            Color::BLACK,
            None,
        )
        .unwrap();
        assert_eq!(pixel(&frame, 12, 12), 0x00ff0000);
        assert_eq!(pixel(&frame, 21, 21), 0x00ff0000);
        assert_eq!(pixel(&frame, 22, 22), 0x00000000);
        assert_eq!(pixel(&frame, 5, 5), 0x00000000);
    }

    #[test]
    fn line_thickness_is_scaled_once_in_device_space() {
        let display = DisplayList::from_commands(vec![
            DrawCommand::TransformPush {
                transform: Transform::scale(Point::new(2.0, 2.0)),
            },
            DrawCommand::Line {
                x1: 10.0,
                y1: 10.0,
                x2: 40.0,
                y2: 10.0,
                width: 4.0,
                color: Color::rgb(0, 255, 0),
            },
            DrawCommand::TransformPop,
        ]);
        // Endpoints become x=20..80 and the width becomes 4*2=8, so the line
        // covers y=16..24. A second transform would double it to 16.
        let frame =
            CpuRasterizer::rasterize(&display, RenderSize::new(96, 48), Color::BLACK).unwrap();
        assert_eq!(pixel(&frame, 50, 20), 0x0000ff00);
        assert_eq!(pixel(&frame, 50, 12), 0x00000000);
        assert_eq!(pixel(&frame, 50, 28), 0x00000000);

        // Non-uniform scales use one average factor, as the GPU capsule does.
        let display = DisplayList::from_commands(vec![
            DrawCommand::TransformPush {
                transform: Transform::scale(Point::new(4.0, 1.0)),
            },
            DrawCommand::Line {
                x1: 10.0,
                y1: 10.0,
                x2: 40.0,
                y2: 10.0,
                width: 4.0,
                color: Color::rgb(0, 255, 0),
            },
            DrawCommand::TransformPop,
        ]);
        // Device width 4*(4+1)/2=10 covers y=5..15.
        let frame =
            CpuRasterizer::rasterize(&display, RenderSize::new(200, 32), Color::BLACK).unwrap();
        assert_eq!(pixel(&frame, 100, 10), 0x0000ff00);
        assert_eq!(pixel(&frame, 100, 2), 0x00000000);
        assert_eq!(pixel(&frame, 100, 18), 0x00000000);
    }

    #[test]
    fn svg_stroke_keeps_local_geometry_under_non_uniform_scale() {
        let display = DisplayList::from_commands(vec![
            DrawCommand::TransformPush {
                transform: Transform::scale(Point::new(4.0, 1.0)),
            },
            DrawCommand::SvgPath {
                x: 0.0,
                y: 0.0,
                width: 16.0,
                height: 8.0,
                path: "M0 0L1 1".to_string(),
                fill: None,
                stroke: Some(Color::rgb(0, 255, 0)),
                stroke_width: 4.0,
            },
            DrawCommand::TransformPop,
        ]);
        let frame =
            CpuRasterizer::rasterize(&display, RenderSize::new(16, 8), Color::BLACK).unwrap();
        // The stroke body covers device (2,0): local (0.625,0.125) lies on the
        // segment, well within the local half width. The shared quad diagonal
        // splits that pixel between two triangles, so assert ink, not a byte.
        assert_ne!(pixel(&frame, 2, 0), 0x00000000);
        // Device (0,4) maps back to local (0,4), 3 units away from the
        // segment: outside the stretched local mask. A device-space capsule
        // with an average-scale width of 10 would wrongly paint it.
        assert_eq!(pixel(&frame, 0, 4), 0x00000000);
    }

    #[test]
    fn bitmap_fallback_still_renders() {
        let display = DisplayList::from_commands(vec![DrawCommand::Text {
            x: 2.0,
            y: 2.0,
            text: "A".to_string(),
            font_size: 16.0,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: Color::WHITE,
            max_width: None,
            alignment: crate::paint::TextAlignment::Start,
        }]);
        let frame = CpuRasterizer::rasterize_with_font(
            &display,
            RenderSize::new(24, 24),
            Color::BLACK,
            None,
        )
        .unwrap();
        assert!(frame.pixels.iter().any(|pixel| *pixel != 0));
        assert_eq!(frame.unsupported_glyphs, 0);
    }

    #[test]
    fn real_font_renders_visible_coverage() {
        let Some(font) = FontRegistry::load_system_default() else {
            return;
        };
        let display = DisplayList::from_commands(vec![DrawCommand::Text {
            x: 2.0,
            y: 2.0,
            text: "M".to_string(),
            font_size: 28.0,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: Color::WHITE,
            max_width: None,
            alignment: crate::paint::TextAlignment::Start,
        }]);
        let frame = CpuRasterizer::rasterize_with_font(
            &display,
            RenderSize::new(40, 40),
            Color::BLACK,
            Some(Arc::new(font)),
        )
        .unwrap();
        assert!(frame.pixels.iter().any(|pixel| *pixel != 0));
        assert_eq!(frame.unsupported_glyphs, 0);
    }

    #[test]
    fn fallback_glyphs_draw_through_the_shared_layout() {
        let Some(stack) = crate::font::default_font_stack() else {
            return;
        };
        if stack.face_for('\u{4f60}', false).is_none() {
            return;
        }
        let display = DisplayList::from_commands(vec![DrawCommand::Text {
            x: 2.0,
            y: 2.0,
            text: "\u{4f60}\u{597d}".to_string(),
            font_size: 24.0,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: Color::WHITE,
            max_width: None,
            alignment: crate::paint::TextAlignment::Start,
        }]);
        let frame =
            CpuRasterizer::rasterize(&display, RenderSize::new(64, 48), Color::BLACK).unwrap();
        assert_eq!(frame.unsupported_glyphs, 0);
        assert!(frame.pixels.iter().any(|pixel| *pixel != 0));
    }

    #[test]
    fn real_font_counts_missing_glyphs() {
        let Some(font) = FontRegistry::load_system_default() else {
            return;
        };
        let display = DisplayList::from_commands(vec![DrawCommand::Text {
            x: 2.0,
            y: 2.0,
            text: "\u{10FFFF}".to_string(),
            font_size: 24.0,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: Color::WHITE,
            max_width: None,
            alignment: crate::paint::TextAlignment::Start,
        }]);
        let frame = CpuRasterizer::rasterize_with_font(
            &display,
            RenderSize::new(32, 32),
            Color::BLACK,
            Some(Arc::new(font)),
        )
        .unwrap();
        assert_eq!(frame.unsupported_glyphs, 1);
    }

    #[test]
    fn rasterizes_filled_svg_path_polygon() {
        let display = DisplayList::from_commands(vec![DrawCommand::SvgPath {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
            path: "M0 0L20 0L20 20L0 20Z".to_string(),
            fill: Some(Color::rgb(255, 0, 0)),
            stroke: None,
            stroke_width: 0.0,
        }]);
        let frame =
            CpuRasterizer::rasterize(&display, RenderSize::new(32, 32), Color::BLACK).unwrap();
        assert_eq!(pixel(&frame, 10, 10), 0x00ff0000);
        assert_eq!(pixel(&frame, 30, 30), 0x00000000);
        assert_eq!(frame.unsupported_commands, 0);
        assert_eq!(frame.command_counts.svg_path, 1);
    }

    #[test]
    fn shadow_draws_around_the_box_but_not_far_away() {
        let display = DisplayList::from_commands(vec![DrawCommand::Shadow {
            x: 20.0,
            y: 20.0,
            width: 40.0,
            height: 40.0,
            radius: 8.0,
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 6.0,
            color: Color::rgb(0, 0, 0),
        }]);
        let frame =
            CpuRasterizer::rasterize(&display, RenderSize::new(100, 100), Color::WHITE).unwrap();
        assert_ne!(pixel(&frame, 16, 40), 0x00ffffff, "blur reaches outside");
        assert_ne!(pixel(&frame, 40, 40), 0x00ffffff, "the casting box is shaded too");
        assert_eq!(pixel(&frame, 90, 90), 0x00ffffff, "far pixels stay clear");
    }

    #[test]
    fn rasterizes_stroked_builtin_icon() {
        let icon = Icon::check();
        let display = DisplayList::from_commands(icon.to_commands(24.0, Color::WHITE));
        let frame =
            CpuRasterizer::rasterize(&display, RenderSize::new(32, 32), Color::BLACK).unwrap();
        assert_eq!(frame.unsupported_commands, 0);
        assert_eq!(frame.command_counts.svg_path, 1);
        let white = frame
            .pixels
            .iter()
            .filter(|pixel| **pixel == 0x00ffffff)
            .count();
        assert!(white > 0, "check icon must produce visible pixels");
    }

    #[test]
    fn damaged_rasterization_matches_a_full_repaint() {
        let previous_display = DisplayList::from_commands(vec![DrawCommand::Rect {
            x: 4.0,
            y: 4.0,
            width: 20.0,
            height: 20.0,
            color: Color::rgb(255, 0, 0),
        }]);
        let next_display = DisplayList::from_commands(vec![
            DrawCommand::Rect {
                x: 4.0,
                y: 4.0,
                width: 20.0,
                height: 20.0,
                color: Color::rgb(255, 0, 0),
            },
            DrawCommand::Rect {
                x: 70.0,
                y: 30.0,
                width: 10.0,
                height: 10.0,
                color: Color::rgb(0, 0, 255),
            },
        ]);
        let size = RenderSize::new(100, 80);
        let previous =
            CpuRasterizer::rasterize(&previous_display, size, Color::BLACK).unwrap();
        let damage = crate::render::diff(&previous_display, &next_display, size);
        let partial =
            CpuRasterizer::rasterize_damaged(&previous, &next_display, size, Color::BLACK, &damage)
                .unwrap();
        let full = CpuRasterizer::rasterize(&next_display, size, Color::BLACK).unwrap();
        assert_eq!(partial.pixels, full.pixels);
        assert!(partial.rasterized_pixels < full.rasterized_pixels);
    }

    #[test]
    fn empty_damage_skips_rasterization_and_resize_forces_full() {
        let display = DisplayList::from_commands(vec![DrawCommand::Rect {
            x: 1.0,
            y: 1.0,
            width: 4.0,
            height: 4.0,
            color: Color::WHITE,
        }]);
        let size = RenderSize::new(32, 32);
        let previous = CpuRasterizer::rasterize(&display, size, Color::BLACK).unwrap();
        let empty = CpuRasterizer::rasterize_damaged(
            &previous,
            &display,
            size,
            Color::BLACK,
            &crate::render::diff(&display, &display, size),
        )
        .unwrap();
        assert_eq!(empty.pixels, previous.pixels);
        assert_eq!(empty.rasterized_pixels, 0);
        assert_eq!(empty.command_counts.total(), 0);

        let resized = CpuRasterizer::rasterize_damaged(
            &previous,
            &display,
            RenderSize::new(40, 40),
            Color::BLACK,
            &crate::render::diff(&display, &display, size),
        )
        .unwrap();
        assert_eq!(resized.width, 40);
        assert_eq!(resized.rasterized_pixels, 1600);
    }

    #[test]
    fn damaged_rasterization_counts_only_unsupported_work_inside_damage() {
        let unresolved_outside = DrawCommand::Image {
            x: 70.0,
            y: 8.0,
            width: 8.0,
            height: 8.0,
            source: ImageHandle::new("outside"),
            source_rect: ImageSourceRect::default(),
            radius: Radius::all(0.0),
            grayscale: false,
        };
        let unresolved_inside = DrawCommand::Image {
            x: 8.0,
            y: 8.0,
            width: 8.0,
            height: 8.0,
            source: ImageHandle::new("inside"),
            source_rect: ImageSourceRect::default(),
            radius: Radius::all(0.0),
            grayscale: false,
        };
        let previous_display = DisplayList::from_commands(vec![unresolved_outside.clone()]);
        let next_display =
            DisplayList::from_commands(vec![unresolved_outside, unresolved_inside]);
        let size = RenderSize::new(96, 48);
        let previous =
            CpuRasterizer::rasterize(&previous_display, size, Color::BLACK).unwrap();
        let damage = crate::render::diff(&previous_display, &next_display, size);
        let partial =
            CpuRasterizer::rasterize_damaged(&previous, &next_display, size, Color::BLACK, &damage)
                .unwrap();
        assert_eq!(partial.unsupported_commands, 1);
    }

    #[test]
    fn glyph_cache_hits_misses_invalidates_and_evicts() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap();
        let font = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
            .ok()
            .and_then(|bytes| FontRegistry::load_bytes(bytes).ok());
        let Some(font) = font else {
            return;
        };
        let font = Arc::new(font);
        let glyph = font.glyph_id('A');
        set_glyph_cache_capacity(DEFAULT_GLYPH_CACHE_CAPACITY);
        let before = glyph_cache_stats();
        let first = cached_glyph(10, &font, glyph, 18.0, false, false);
        let second = cached_glyph(10, &font, glyph, 18.0, false, false);
        assert_eq!(first.coverage, second.coverage);
        let hit = glyph_cache_stats();
        assert!(hit.hits > before.hits);
        assert!(hit.misses > before.misses);

        let before_stack = glyph_cache_stats();
        let _other = cached_glyph(11, &font, glyph, 18.0, false, false);
        assert!(glyph_cache_stats().misses > before_stack.misses);

        set_glyph_cache_capacity(1);
        let _second = cached_glyph(11, &font, font.glyph_id('B'), 20.0, true, false);
        assert!(glyph_cache_stats().entries <= 1);
        set_glyph_cache_capacity(DEFAULT_GLYPH_CACHE_CAPACITY);
    }

    #[test]
    fn disabling_glyph_cache_preserves_pixels() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap();
        let Some(font) = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
            .ok()
            .and_then(|bytes| FontRegistry::load_bytes(bytes).ok())
        else {
            return;
        };
        let font = Arc::new(font);
        let display = DisplayList::from_commands(vec![DrawCommand::Text {
            x: 2.0,
            y: 2.0,
            text: "cache parity".to_string(),
            font_size: 18.0,
            weight: 700,
            italic: true,
            mono: false,
            family: None,
            color: Color::WHITE,
            max_width: None,
            alignment: TextAlignment::Start,
        }]);
        set_glyph_cache_enabled(true);
        let cached = CpuRasterizer::rasterize_with_font(
            &display,
            RenderSize::new(180, 40),
            Color::BLACK,
            Some(font.clone()),
        )
        .unwrap();
        set_glyph_cache_enabled(false);
        let uncached = CpuRasterizer::rasterize_with_font(
            &display,
            RenderSize::new(180, 40),
            Color::BLACK,
            Some(font),
        )
        .unwrap();
        set_glyph_cache_enabled(true);
        assert_eq!(cached.pixels, uncached.pixels);
    }

    #[test]
    fn damage_prefilter_matches_unfiltered_rasterization() {
        let previous_display = DisplayList::from_commands(vec![
            DrawCommand::Rect {
                x: 4.0,
                y: 4.0,
                width: 12.0,
                height: 12.0,
                color: Color::rgb(255, 0, 0),
            },
            DrawCommand::TransformPush {
                transform: Transform::translate(Point::new(30.0, 0.0)),
            },
            DrawCommand::Rect {
                x: 40.0,
                y: 40.0,
                width: 8.0,
                height: 8.0,
                color: Color::rgb(0, 255, 0),
            },
            DrawCommand::TransformPop,
        ]);
        let next_display = DisplayList::from_commands(vec![
            DrawCommand::Rect {
                x: 4.0,
                y: 4.0,
                width: 12.0,
                height: 12.0,
                color: Color::rgb(255, 0, 0),
            },
            DrawCommand::TransformPush {
                transform: Transform::translate(Point::new(30.0, 0.0)),
            },
            DrawCommand::Rect {
                x: 40.0,
                y: 40.0,
                width: 8.0,
                height: 8.0,
                color: Color::rgb(0, 255, 0),
            },
            DrawCommand::TransformPop,
            DrawCommand::Rect {
                x: 60.0,
                y: 60.0,
                width: 6.0,
                height: 6.0,
                color: Color::rgb(0, 0, 255),
            },
        ]);
        let size = RenderSize::new(96, 80);
        let previous =
            CpuRasterizer::rasterize(&previous_display, size, Color::BLACK).unwrap();
        let damage = crate::render::diff(&previous_display, &next_display, size);
        let filtered =
            CpuRasterizer::rasterize_damaged(&previous, &next_display, size, Color::BLACK, &damage)
                .unwrap();
        let unfiltered = CpuRasterizer::rasterize_damaged_inner(
            &previous,
            &next_display,
            size,
            Color::BLACK,
            &damage,
            false,
        )
        .unwrap();
        let full = CpuRasterizer::rasterize(&next_display, size, Color::BLACK).unwrap();
        assert_eq!(filtered.pixels, unfiltered.pixels);
        assert_eq!(filtered.pixels, full.pixels);
        assert!(filtered.command_counts.total() < unfiltered.command_counts.total());
    }

    #[test]
    fn prefilter_skips_outside_and_scissored_commands() {
        let display = DisplayList::from_commands(vec![
            DrawCommand::Rect {
                x: 2.0,
                y: 2.0,
                width: 4.0,
                height: 4.0,
                color: Color::WHITE,
            },
            DrawCommand::Rect {
                x: 70.0,
                y: 2.0,
                width: 4.0,
                height: 4.0,
                color: Color::WHITE,
            },
            DrawCommand::ScissorPush {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            },
            DrawCommand::Rect {
                x: 60.0,
                y: 2.0,
                width: 4.0,
                height: 4.0,
                color: Color::WHITE,
            },
            DrawCommand::ScissorPop,
        ]);
        let filtered = prefilter_display(&display, Rect::new(0.0, 0.0, 20.0, 20.0));
        assert_eq!(filtered.commands.iter().filter(|command| matches!(command, DrawCommand::Rect { .. })).count(), 1);
        assert!(filtered.commands.iter().any(|command| matches!(command, DrawCommand::ScissorPush { .. })));
        assert!(filtered.commands.iter().any(|command| matches!(command, DrawCommand::ScissorPop)));
    }
}
