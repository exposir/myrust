//! Small CPU presenter for native windows.
//!
//! This is intentionally separate from [`crate::SoftwareRenderer`],
//! which remains a non-rasterizing command analyzer. The presenter draws basic
//! geometry and prefers a system TTF through `ab_glyph`, falling back to a
//! fixed 8x8 ASCII bitmap font. It is not a general shaping engine or image
//! decoder.

use font8x8::{UnicodeFonts, BASIC_FONTS};
use std::sync::Arc;

use super::font::{default_font, FontFace, GlyphBitmap};
use super::layout::Point;
use super::paint::{Color, DisplayList, DrawCommand, GradientStop, Radius, Rect, TextAlignment};
use super::render::{color_to_array, intersect_rects, CommandCounts, RenderSize, TransformMatrix};
use super::svg::{parse_svg_path, SvgContour};

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
    /// Image commands that were not drawn by the presenter.
    pub unsupported_commands: usize,
    /// Glyphs missing from the active real font or bitmap fallback.
    pub unsupported_glyphs: usize,
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
    /// gradients, SVG path fills/strokes, transforms, and scissors. Text uses
    /// the process-wide TTF font when available and falls back to the built-in
    /// 8x8 bitmap font. Images are still counted as unsupported because the
    /// display list carries only a source handle, not decoded pixels. There is
    /// no shaping, kerning, bidi, font fallback, alignment, or wrapping here.
    pub fn rasterize(
        display_list: &DisplayList,
        size: RenderSize,
        background: Color,
    ) -> Result<CpuFrame, RasterError> {
        Self::rasterize_with_font(display_list, size, background, default_font())
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
        Self::rasterize_inner(display_list, size, background, font, false).map(|(frame, _)| frame)
    }

    /// Like [`rasterize_with_font`] but skips text drawing and returns collected
    /// text commands for browser-native canvas2D rendering.
    pub fn rasterize_with_font_collect_text(
        display_list: &DisplayList,
        size: RenderSize,
        background: Color,
        font: Option<Arc<FontFace>>,
    ) -> Result<(CpuFrame, Vec<TextRenderCommand>), RasterError> {
        Self::rasterize_inner(display_list, size, background, font, true)
    }

    fn rasterize_inner(
        display_list: &DisplayList,
        size: RenderSize,
        background: Color,
        font: Option<Arc<FontFace>>,
        collect_text: bool,
    ) -> Result<(CpuFrame, Vec<TextRenderCommand>), RasterError> {
        if size.is_empty() {
            return Err(RasterError::InvalidSize {
                width: size.width,
                height: size.height,
            });
        }
        let mut canvas = Canvas::new(size, background, font);
        canvas.collect_text_only = collect_text;
        let mut transform = TransformMatrix::IDENTITY;
        let mut transform_stack = Vec::new();
        let mut scissor = None;
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
                    let mut draw_x = *x;
                    // The rasterizer has no shaping or wrapping; still honor
                    // single-line alignment inside a finite max width so nodes
                    // that center labels draw them centered.
                    if let Some(max_width) = *max_width {
                        if max_width.is_finite() && max_width > 0.0 {
                            let widest = text
                                .split('\n')
                                .map(|line| canvas.measure_text(line, *font_size))
                                .fold(0.0_f32, f32::max);
                            if widest <= max_width {
                                draw_x += match alignment {
                                    super::paint::TextAlignment::Center => {
                                        (max_width - widest) / 2.0
                                    }
                                    super::paint::TextAlignment::End => max_width - widest,
                                    super::paint::TextAlignment::Start => 0.0,
                                };
                            }
                        }
                    }
                    if collect_text {
                        let line_height = canvas.line_height(*font_size);
                        for (index, line) in text.split('\n').enumerate() {
                            canvas.text_commands.push(TextRenderCommand {
                                x: draw_x,
                                y: *y + index as f32 * line_height,
                                text: line.to_string(),
                                font_size: *font_size,
                                weight: *weight,
                                italic: *italic,
                                mono: *mono,
                                family: family.clone(),
                                color: *color,
                                max_width: *max_width,
                                alignment: *alignment,
                                transform: transform.to_array(),
                                clip: scissor.map(|rect| {
                                    [rect.x, rect.y, rect.width, rect.height]
                                }),
                            });
                        }
                    } else {
                        canvas.draw_text(
                            transform,
                            scissor,
                            Point::new(draw_x, *y),
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
                DrawCommand::Image { .. } => {}
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
        let unsupported_commands = canvas.command_counts.image;
        Ok((
            CpuFrame {
                width: size.width,
                height: size.height,
                pixels: canvas.pixels,
                command_counts: canvas.command_counts,
                unsupported_commands,
                unsupported_glyphs: canvas.unsupported_glyphs,
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
    font: Option<Arc<FontFace>>,
    text_commands: Vec<TextRenderCommand>,
    collect_text_only: bool,
}

impl Canvas {
    fn new(size: RenderSize, background: Color, font: Option<Arc<FontFace>>) -> Self {
        let packed = pack_color(background);
        Self {
            width: size.width,
            height: size.height,
            pixels: vec![packed; size.width as usize * size.height as usize],
            command_counts: CommandCounts::default(),
            unsupported_glyphs: 0,
            font,
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
                let bitmap = font.rasterize(character, font_size);
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

fn pack_color(color: Color) -> u32 {
    (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::FontRegistry;
    use crate::icon::Icon;
    use crate::paint::Transform;

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
}
