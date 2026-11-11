//! Small CPU presenter for native windows.
//!
//! This is intentionally separate from [`crate::SoftwareRenderer`],
//! which remains a non-rasterizing command analyzer. The presenter draws basic
//! geometry and prefers a system TTF through `ab_glyph`, falling back to a
//! fixed 8x8 ASCII bitmap font. It is not a general shaping engine or image
//! decoder.

use font8x8::{UnicodeFonts, BASIC_FONTS};
use std::sync::Arc;

use super::font::{default_font, FontFace};
use super::layout::Point;
use super::paint::{Color, DisplayList, DrawCommand, GradientStop, Radius, Rect};
use super::render::{color_to_array, intersect_rects, CommandCounts, RenderSize, TransformMatrix};
use super::svg::{parse_svg_path, SvgContour};

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
        if size.is_empty() {
            return Err(RasterError::InvalidSize {
                width: size.width,
                height: size.height,
            });
        }
        let mut canvas = Canvas::new(size, background, font);
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
                    color,
                    ..
                } => {
                    canvas.draw_text(
                        transform,
                        scissor,
                        Point::new(*x, *y),
                        text,
                        *font_size,
                        *color,
                    );
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
                    } => canvas.draw_line(
                        transform,
                        scissor,
                        Point::new(*x1, *y1),
                        Point::new(*x2, *y2),
                        *width,
                        *color,
                    ),
                    DrawCommand::LinearGradient {
                        x,
                        y,
                        width,
                        height,
                        start,
                        end,
                        stops,
                    } => canvas.fill_gradient(
                        transform,
                        scissor,
                        Rect::new(*x, *y, *width, *height),
                        *start,
                        *end,
                        stops,
                    ),
                    _ => {}
                },
            }
        }

        let unsupported_commands = canvas.command_counts.image;
        Ok(CpuFrame {
            width: size.width,
            height: size.height,
            pixels: canvas.pixels,
            command_counts: canvas.command_counts,
            unsupported_commands,
            unsupported_glyphs: canvas.unsupported_glyphs,
        })
    }
}

struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
    command_counts: CommandCounts,
    unsupported_glyphs: usize,
    font: Option<Arc<FontFace>>,
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
        }
    }

    fn fill_rect(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        rect: Rect,
        color: Color,
    ) {
        let points = [
            Point::new(rect.x, rect.y),
            Point::new(rect.right(), rect.y),
            Point::new(rect.right(), rect.bottom()),
            Point::new(rect.x, rect.bottom()),
        ];
        let color = color_to_array(color);
        self.fill_triangle(
            transform, scissor, points[0], points[1], points[2], [color; 3],
        );
        self.fill_triangle(
            transform, scissor, points[0], points[2], points[3], [color; 3],
        );
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

    fn draw_line(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        start: Point,
        end: Point,
        width: f32,
        color: Color,
    ) {
        if width <= 0.0 {
            return;
        }
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = (dx * dx + dy * dy).sqrt();
        if length <= f32::EPSILON {
            return;
        }
        let half = width / 2.0;
        let normal = Point::new(-dy / length * half, dx / length * half);
        let points = [
            Point::new(start.x + normal.x, start.y + normal.y),
            Point::new(end.x + normal.x, end.y + normal.y),
            Point::new(end.x - normal.x, end.y - normal.y),
            Point::new(start.x - normal.x, start.y - normal.y),
        ];
        let color = color_to_array(color);
        self.fill_triangle(
            transform, scissor, points[0], points[1], points[2], [color; 3],
        );
        self.fill_triangle(
            transform, scissor, points[0], points[2], points[3], [color; 3],
        );
    }

    fn fill_gradient(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        rect: Rect,
        start: Point,
        end: Point,
        stops: &[GradientStop],
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 || stops.is_empty() {
            return;
        }
        let mut ordered = stops.to_vec();
        ordered.sort_by(|left, right| left.offset.total_cmp(&right.offset));
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

    fn draw_text(
        &mut self,
        transform: TransformMatrix,
        scissor: Option<Rect>,
        origin: Point,
        text: &str,
        font_size: f32,
        color: Color,
    ) {
        let Some(font) = self.font.clone() else {
            self.draw_bitmap_text(transform, scissor, origin, text, font_size, color);
            return;
        };
        let font_size = font_size.max(1.0);
        let line_height = font.line_height(font_size);
        let baseline_offset = font.ascent(font_size);
        let color = color_to_array(color);
        for (line_index, line) in text.split('\n').enumerate() {
            let mut cursor_x = origin.x;
            let baseline_y = origin.y + line_index as f32 * line_height + baseline_offset;
            for character in line.chars() {
                if !font.has_glyph(character) {
                    self.unsupported_glyphs += 1;
                    cursor_x += font_size * 0.5;
                    continue;
                }
                let bitmap = font.rasterize(character, font_size);
                for row in 0..bitmap.height {
                    for column in 0..bitmap.width {
                        let coverage =
                            bitmap.coverage[(row * bitmap.width + column) as usize] as f32 / 255.0;
                        if coverage <= 0.0 {
                            continue;
                        }
                        let x = cursor_x + bitmap.bearing_x as f32 + column as f32;
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
                cursor_x += font.glyph_advance(character, font_size);
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
        for y in min_y..max_y {
            for x in min_x..max_x {
                let point = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                let weights = [
                    edge(b, c, point) * sign,
                    edge(c, a, point) * sign,
                    edge(a, b, point) * sign,
                ];
                if weights.iter().all(|weight| *weight >= -0.0001) {
                    let normalized = [
                        weights[0] * inverse_area,
                        weights[1] * inverse_area,
                        weights[2] * inverse_area,
                    ];
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
        let width = stroke_width * transform.scale_factor();
        if width <= 0.0 {
            return;
        }
        for contour in contours {
            if contour.points.len() < 2 {
                continue;
            }
            for pair in contour.points.windows(2) {
                self.draw_line(transform, scissor, pair[0], pair[1], width, color);
            }
            if contour.closed {
                let first = contour.points[0];
                let last = *contour.points.last().expect("contour has points");
                self.draw_line(transform, scissor, last, first, width, color);
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

fn add_arc(points: &mut Vec<Point>, center: Point, radius: f32, start: f32, end: f32) {
    for segment in 0..=6 {
        let angle = start + (end - start) * segment as f32 / 6.0;
        points.push(Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
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
    fn bitmap_fallback_still_renders() {
        let display = DisplayList::from_commands(vec![DrawCommand::Text {
            x: 2.0,
            y: 2.0,
            text: "A".to_string(),
            font_size: 16.0,
            weight: 400,
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
