//! Serializable drawing instructions and display-list geometry.
//!
//! This module only builds and transforms CPU-side command data. It does not
//! rasterize pixels, decode images, shape fonts, or initialize a GPU backend.

use serde::{Deserialize, Serialize};

use super::layout::Point;
use super::text::{layout_text, ApproximateMeasurer};

/// RGBA color represented by four 8-bit channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel, where 0 is transparent and 255 is opaque.
    pub a: u8,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    /// Creates an opaque color from red, green, and blue channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }

    /// Creates a color from all four channels.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parses `#RGB`, `#RGBA`, `#RRGGBB`, or `#RRGGBBAA`.
    ///
    /// The leading `#` is optional. Three- and four-digit shorthand expands
    /// each nibble by repeating it, so `#f80` becomes `#ff8800`.
    pub fn from_hex(value: &str) -> Result<Self, ColorParseError> {
        let raw = value.trim().strip_prefix('#').unwrap_or(value.trim());
        if !raw.is_ascii() {
            return Err(ColorParseError::InvalidDigit);
        }
        let parse_pair =
            |text: &str| u8::from_str_radix(text, 16).map_err(|_| ColorParseError::InvalidDigit);
        let expand_nibble = |text: &str| {
            let nibble = parse_pair(text)?;
            Ok::<u8, ColorParseError>(nibble * 17)
        };

        match raw.len() {
            3 => Ok(Self::rgb(
                expand_nibble(&raw[0..1])?,
                expand_nibble(&raw[1..2])?,
                expand_nibble(&raw[2..3])?,
            )),
            4 => Ok(Self::rgba(
                expand_nibble(&raw[0..1])?,
                expand_nibble(&raw[1..2])?,
                expand_nibble(&raw[2..3])?,
                expand_nibble(&raw[3..4])?,
            )),
            6 => Ok(Self::rgb(
                parse_pair(&raw[0..2])?,
                parse_pair(&raw[2..4])?,
                parse_pair(&raw[4..6])?,
            )),
            8 => Ok(Self::rgba(
                parse_pair(&raw[0..2])?,
                parse_pair(&raw[2..4])?,
                parse_pair(&raw[4..6])?,
                parse_pair(&raw[6..8])?,
            )),
            length => Err(ColorParseError::InvalidLength { length }),
        }
    }

    /// Formats the color as an uppercase `#RRGGBBAA` string.
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
    }
}

impl From<[u8; 4]> for Color {
    fn from([r, g, b, a]: [u8; 4]) -> Self {
        Self::rgba(r, g, b, a)
    }
}

/// Error returned when a hexadecimal color cannot be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ColorParseError {
    /// The input did not contain 3, 4, 6, or 8 hexadecimal digits.
    #[error("color must contain 3, 4, 6, or 8 hexadecimal digits")]
    InvalidLength {
        /// Number of digits supplied after the optional `#`.
        length: usize,
    },
    /// At least one character was not a hexadecimal digit.
    #[error("color contains a non-hexadecimal digit")]
    InvalidDigit,
}

/// Axis-aligned rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Rectangle width.
    pub width: f32,
    /// Rectangle height.
    pub height: f32,
}

impl Rect {
    /// Creates a rectangle.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns the right edge.
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    /// Returns the bottom edge.
    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    /// Returns whether the rectangle has a finite, non-negative area.
    pub fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
    }

    /// Returns the smallest rectangle containing both inputs.
    pub fn union(self, other: Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self::new(left, top, right - left, bottom - top)
    }

    /// Returns the overlapping rectangle, if the rectangles intersect.
    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right >= left && bottom >= top).then(|| Self::new(left, top, right - left, bottom - top))
    }
}

/// Four independent corner radii.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Radius {
    /// Top-left radius.
    pub top_left: f32,
    /// Top-right radius.
    pub top_right: f32,
    /// Bottom-right radius.
    pub bottom_right: f32,
    /// Bottom-left radius.
    pub bottom_left: f32,
}

impl Radius {
    /// Creates independent corner radii.
    pub const fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    /// Creates one radius for every corner.
    pub const fn all(radius: f32) -> Self {
        Self::new(radius, radius, radius, radius)
    }
}

/// Text horizontal alignment within an optional maximum width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextAlignment {
    /// Align text to the start edge.
    #[default]
    Start,
    /// Center text within its maximum width.
    Center,
    /// Align text to the end edge.
    End,
}

/// Opaque identifier for an image supplied by a later renderer or decoder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImageHandle(String);

impl ImageHandle {
    /// Creates an image handle from a stable source identifier.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the source identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Gradient color stop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position in the range `0.0..=1.0`.
    pub offset: f32,
    /// Color at this position.
    pub color: Color,
}

/// Reusable SVG path styling payload.
///
/// Path data is retained as text. This batch does not parse, tessellate, or
/// rasterize it; a later renderer owns those operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SvgPath {
    /// SVG path data.
    pub path: String,
    /// Optional fill color.
    pub fill: Option<Color>,
    /// Optional stroke color.
    pub stroke: Option<Color>,
    /// Stroke width in SVG user units.
    pub stroke_width: f32,
}

impl SvgPath {
    /// Creates an unstyled SVG path.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            fill: None,
            stroke: None,
            stroke_width: 0.0,
        }
    }

    /// Creates a stroked SVG path.
    pub fn stroked(path: impl Into<String>, stroke_width: f32, color: Color) -> Self {
        Self {
            path: path.into(),
            fill: None,
            stroke: Some(color),
            stroke_width,
        }
    }

    /// Sets the fill color.
    pub fn with_fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    /// Sets the stroke color.
    pub fn with_stroke(mut self, color: Color, width: f32) -> Self {
        self.stroke = Some(color);
        self.stroke_width = width;
        self
    }
}

/// Local affine transform applied to subsequent drawing commands.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Translation applied after scaling and rotation.
    pub translation: Point,
    /// Horizontal and vertical scale factors.
    pub scale: Point,
    /// Rotation in radians.
    pub rotation: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}

impl Transform {
    /// Creates the identity transform.
    pub const fn identity() -> Self {
        Self {
            translation: Point::new(0.0, 0.0),
            scale: Point::new(1.0, 1.0),
            rotation: 0.0,
        }
    }

    /// Creates a translation-only transform.
    pub const fn translate(offset: Point) -> Self {
        Self {
            translation: offset,
            scale: Point::new(1.0, 1.0),
            rotation: 0.0,
        }
    }

    /// Creates a scale-only transform.
    pub const fn scale(scale: Point) -> Self {
        Self {
            translation: Point::new(0.0, 0.0),
            scale,
            rotation: 0.0,
        }
    }
}

/// One serializable native-UI drawing instruction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DrawCommand {
    /// Axis-aligned filled rectangle.
    Rect {
        /// Local x coordinate.
        x: f32,
        /// Local y coordinate.
        y: f32,
        /// Rectangle width.
        width: f32,
        /// Rectangle height.
        height: f32,
        /// Fill color.
        color: Color,
    },
    /// Filled rectangle with independently rounded corners.
    RoundedRect {
        /// Local x coordinate.
        x: f32,
        /// Local y coordinate.
        y: f32,
        /// Rectangle width.
        width: f32,
        /// Rectangle height.
        height: f32,
        /// Corner radii.
        radius: Radius,
        /// Fill color.
        color: Color,
    },
    /// Filled circle.
    Circle {
        /// Center x coordinate.
        x: f32,
        /// Center y coordinate.
        y: f32,
        /// Circle radius.
        radius: f32,
        /// Fill color.
        color: Color,
    },
    /// Straight stroked line.
    Line {
        /// Start x coordinate.
        x1: f32,
        /// Start y coordinate.
        y1: f32,
        /// End x coordinate.
        x2: f32,
        /// End y coordinate.
        y2: f32,
        /// Stroke width.
        width: f32,
        /// Stroke color.
        color: Color,
    },
    /// Text produced from caller-provided strings.
    ///
    /// No shaping, kerning, or fallback-font selection occurs in this batch.
    Text {
        /// Local x coordinate.
        x: f32,
        /// Local top y coordinate.
        y: f32,
        /// UTF-8 text.
        text: String,
        /// Font size in logical pixels.
        font_size: f32,
        /// Numeric font weight, conventionally `100..=900`.
        weight: u16,
        /// Text color.
        color: Color,
        /// Optional wrapping and alignment width.
        max_width: Option<f32>,
        /// Horizontal alignment within `max_width`.
        alignment: TextAlignment,
    },
    /// SVG path data in a local viewport.
    ///
    /// Parsing and rendering are intentionally deferred.
    SvgPath {
        /// Local viewport x coordinate.
        x: f32,
        /// Local viewport y coordinate.
        y: f32,
        /// Local viewport width.
        width: f32,
        /// Local viewport height.
        height: f32,
        /// SVG path data.
        path: String,
        /// Optional fill color.
        fill: Option<Color>,
        /// Optional stroke color.
        stroke: Option<Color>,
        /// Stroke width in SVG user units.
        stroke_width: f32,
    },
    /// Axis-aligned linear gradient.
    LinearGradient {
        /// Local x coordinate.
        x: f32,
        /// Local y coordinate.
        y: f32,
        /// Gradient rectangle width.
        width: f32,
        /// Gradient rectangle height.
        height: f32,
        /// Normalized start point in `0.0..=1.0`.
        start: Point,
        /// Normalized end point in `0.0..=1.0`.
        end: Point,
        /// Ordered color stops.
        stops: Vec<GradientStop>,
    },
    /// Image drawing instruction. Image decoding and sampling are not implemented.
    Image {
        /// Local x coordinate.
        x: f32,
        /// Local y coordinate.
        y: f32,
        /// Image width.
        width: f32,
        /// Image height.
        height: f32,
        /// Opaque image source handle.
        source: ImageHandle,
    },
    /// Begins clipping subsequent commands to a rectangle.
    ScissorPush {
        /// Clip rectangle in the current coordinate space.
        rect: Rect,
    },
    /// Restores the previous scissor rectangle.
    ScissorPop,
    /// Begins a nested affine transform.
    TransformPush {
        /// Transform applied to subsequent commands.
        transform: Transform,
    },
    /// Restores the previous transform.
    TransformPop,
}

/// Ordered drawing instructions with CPU-side geometry helpers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DisplayList {
    /// Instructions in paint order.
    pub commands: Vec<DrawCommand>,
}

impl DisplayList {
    /// Creates an empty display list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a display list from existing commands.
    pub fn from_commands(commands: Vec<DrawCommand>) -> Self {
        Self { commands }
    }

    /// Appends one command.
    pub fn push(&mut self, command: DrawCommand) {
        self.commands.push(command);
    }

    /// Appends a sequence of commands in order.
    pub fn extend(&mut self, commands: impl IntoIterator<Item = DrawCommand>) {
        self.commands.extend(commands);
    }

    /// Removes every command.
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Returns the command count.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Returns whether the list contains no commands.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Computes the union of all rendered geometry bounds.
    ///
    /// Transform push/pop commands are composed while walking the list. A
    /// scissor does not contribute its own rectangle because it is a state
    /// instruction, not painted geometry.
    pub fn bounds(&self) -> Option<Rect> {
        let mut current = Affine::IDENTITY;
        let mut stack = Vec::new();
        let mut bounds: Option<Rect> = None;

        for command in &self.commands {
            match command {
                DrawCommand::TransformPush { transform } => {
                    stack.push(current);
                    current = current.then(transform.affine());
                }
                DrawCommand::TransformPop => {
                    if let Some(previous) = stack.pop() {
                        current = previous;
                    }
                }
                DrawCommand::Rect {
                    x,
                    y,
                    width,
                    height,
                    ..
                }
                | DrawCommand::RoundedRect {
                    x,
                    y,
                    width,
                    height,
                    ..
                }
                | DrawCommand::Image {
                    x,
                    y,
                    width,
                    height,
                    ..
                }
                | DrawCommand::LinearGradient {
                    x,
                    y,
                    width,
                    height,
                    ..
                }
                | DrawCommand::SvgPath {
                    x,
                    y,
                    width,
                    height,
                    ..
                } => include_rect(
                    &mut bounds,
                    current.transform_rect(Rect::new(*x, *y, *width, *height)),
                ),
                DrawCommand::Circle { x, y, radius, .. } => include_rect(
                    &mut bounds,
                    current.transform_rect(Rect::new(
                        *x - *radius,
                        *y - *radius,
                        *radius * 2.0,
                        *radius * 2.0,
                    )),
                ),
                DrawCommand::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                    ..
                } => {
                    let half_width = width.max(0.0) / 2.0;
                    include_rect(
                        &mut bounds,
                        current.transform_rect(Rect::new(
                            x1.min(*x2) - half_width,
                            y1.min(*y2) - half_width,
                            (x1 - x2).abs() + half_width * 2.0,
                            (y1 - y2).abs() + half_width * 2.0,
                        )),
                    );
                }
                DrawCommand::Text {
                    x,
                    y,
                    text,
                    font_size,
                    max_width,
                    alignment,
                    ..
                } => {
                    let lines = layout_text(text, *font_size, *max_width, &ApproximateMeasurer);
                    let measured_width =
                        lines.iter().map(|line| line.width).fold(0.0_f32, f32::max);
                    let measured_height = lines.iter().map(|line| line.line_height).sum();
                    let maximum =
                        max_width.and_then(|value| value.is_finite().then_some(value.max(0.0)));
                    let width = maximum
                        .map(|value| measured_width.min(value))
                        .unwrap_or(measured_width);
                    let x = match (*alignment, maximum) {
                        (TextAlignment::Center, Some(maximum)) => *x + (maximum - width) / 2.0,
                        (TextAlignment::End, Some(maximum)) => *x + maximum - width,
                        _ => *x,
                    };
                    include_rect(
                        &mut bounds,
                        current.transform_rect(Rect::new(x, *y, width, measured_height)),
                    );
                }
                DrawCommand::ScissorPush { .. } | DrawCommand::ScissorPop => {}
            }
        }

        bounds
    }

    /// Translates every existing command by a logical-pixel offset.
    ///
    /// Lists that already contain transform scopes are wrapped in an outer
    /// transform so nested scaling or rotation remains correct. Other lists
    /// are translated in place to keep command counts stable.
    pub fn translate(&mut self, offset: Point) {
        if self.commands.is_empty() || (offset.x == 0.0 && offset.y == 0.0) {
            return;
        }
        if self
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::TransformPush { .. }))
        {
            self.commands.insert(
                0,
                DrawCommand::TransformPush {
                    transform: Transform::translate(offset),
                },
            );
            self.commands.push(DrawCommand::TransformPop);
            return;
        }

        for command in &mut self.commands {
            match command {
                DrawCommand::Rect { x, y, .. }
                | DrawCommand::RoundedRect { x, y, .. }
                | DrawCommand::Circle { x, y, .. }
                | DrawCommand::Text { x, y, .. }
                | DrawCommand::SvgPath { x, y, .. }
                | DrawCommand::LinearGradient { x, y, .. }
                | DrawCommand::Image { x, y, .. } => {
                    *x += offset.x;
                    *y += offset.y;
                }
                DrawCommand::Line { x1, y1, x2, y2, .. } => {
                    *x1 += offset.x;
                    *y1 += offset.y;
                    *x2 += offset.x;
                    *y2 += offset.y;
                }
                DrawCommand::ScissorPush { rect } => {
                    rect.x += offset.x;
                    rect.y += offset.y;
                }
                DrawCommand::TransformPush { .. }
                | DrawCommand::ScissorPop
                | DrawCommand::TransformPop => {}
            }
        }
    }

    /// Wraps the current command sequence in a scissor push/pop pair.
    pub fn clip(&mut self, rect: Rect) {
        self.commands.insert(0, DrawCommand::ScissorPush { rect });
        self.commands.push(DrawCommand::ScissorPop);
    }
}

#[derive(Debug, Clone, Copy)]
struct Affine {
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
    tx: f32,
    ty: f32,
}

impl Affine {
    const IDENTITY: Self = Self {
        m11: 1.0,
        m12: 0.0,
        m21: 0.0,
        m22: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    fn apply(self, point: Point) -> Point {
        Point::new(
            self.m11 * point.x + self.m12 * point.y + self.tx,
            self.m21 * point.x + self.m22 * point.y + self.ty,
        )
    }

    fn then(self, child: Self) -> Self {
        Self {
            m11: self.m11 * child.m11 + self.m12 * child.m21,
            m12: self.m11 * child.m12 + self.m12 * child.m22,
            m21: self.m21 * child.m11 + self.m22 * child.m21,
            m22: self.m21 * child.m12 + self.m22 * child.m22,
            tx: self.m11 * child.tx + self.m12 * child.ty + self.tx,
            ty: self.m21 * child.tx + self.m22 * child.ty + self.ty,
        }
    }

    fn transform_rect(self, rect: Rect) -> Rect {
        let points = [
            self.apply(Point::new(rect.x, rect.y)),
            self.apply(Point::new(rect.right(), rect.y)),
            self.apply(Point::new(rect.right(), rect.bottom())),
            self.apply(Point::new(rect.x, rect.bottom())),
        ];
        let left = points
            .iter()
            .map(|point| point.x)
            .fold(f32::INFINITY, f32::min);
        let top = points
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);
        let right = points
            .iter()
            .map(|point| point.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = points
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max);
        Rect::new(left, top, right - left, bottom - top)
    }
}

impl Transform {
    fn affine(self) -> Affine {
        let cos = self.rotation.cos();
        let sin = self.rotation.sin();
        Affine {
            m11: cos * self.scale.x,
            m12: -sin * self.scale.y,
            m21: sin * self.scale.x,
            m22: cos * self.scale.y,
            tx: self.translation.x,
            ty: self.translation.y,
        }
    }
}

fn include_rect(bounds: &mut Option<Rect>, rect: Rect) {
    if !rect.is_valid() {
        return;
    }
    *bounds = Some(match *bounds {
        Some(current) => current.union(rect),
        None => rect,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors_in_supported_formats() {
        assert_eq!(Color::from_hex("#f80").unwrap(), Color::rgb(255, 136, 0));
        assert_eq!(
            Color::from_hex("336699CC").unwrap(),
            Color::rgba(0x33, 0x66, 0x99, 0xcc)
        );
        assert_eq!(
            Color::from_hex("#abcd").unwrap(),
            Color::rgba(0xaa, 0xbb, 0xcc, 0xdd)
        );
        assert_eq!(
            Color::from_hex("#12345g").unwrap_err(),
            ColorParseError::InvalidDigit
        );
        assert_eq!(
            Color::from_hex("#12345").unwrap_err(),
            ColorParseError::InvalidLength { length: 5 }
        );
        assert_eq!(
            Color::from_hex("éa").unwrap_err(),
            ColorParseError::InvalidDigit
        );
    }

    #[test]
    fn display_list_bounds_union_all_geometry() {
        let list = DisplayList::from_commands(vec![
            DrawCommand::Rect {
                x: 10.0,
                y: 20.0,
                width: 30.0,
                height: 10.0,
                color: Color::WHITE,
            },
            DrawCommand::Circle {
                x: 70.0,
                y: 50.0,
                radius: 5.0,
                color: Color::WHITE,
            },
        ]);
        assert_eq!(list.bounds(), Some(Rect::new(10.0, 20.0, 65.0, 35.0)));
    }

    #[test]
    fn translate_updates_bounds_and_existing_commands() {
        let mut list = DisplayList::from_commands(vec![DrawCommand::Rect {
            x: 2.0,
            y: 3.0,
            width: 8.0,
            height: 5.0,
            color: Color::WHITE,
        }]);
        list.translate(Point::new(10.0, 20.0));
        assert_eq!(list.bounds(), Some(Rect::new(12.0, 23.0, 8.0, 5.0)));
    }

    #[test]
    fn translate_composes_with_existing_transform_once() {
        let mut list = DisplayList::from_commands(vec![
            DrawCommand::TransformPush {
                transform: Transform::scale(Point::new(2.0, 2.0)),
            },
            DrawCommand::Rect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 10.0,
                color: Color::WHITE,
            },
            DrawCommand::TransformPop,
        ]);
        assert_eq!(list.bounds(), Some(Rect::new(0.0, 0.0, 40.0, 20.0)));
        list.translate(Point::new(10.0, 20.0));
        assert_eq!(list.bounds(), Some(Rect::new(10.0, 20.0, 40.0, 20.0)));
    }

    #[test]
    fn clip_wraps_the_existing_command_sequence() {
        let mut list = DisplayList::from_commands(vec![DrawCommand::Rect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
            color: Color::WHITE,
        }]);
        list.clip(Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(list.len(), 3);
        assert!(matches!(list.commands[0], DrawCommand::ScissorPush { .. }));
        assert!(matches!(list.commands[2], DrawCommand::ScissorPop));
    }

    #[test]
    fn display_list_round_trips_through_serde_json() {
        let list = DisplayList::from_commands(vec![DrawCommand::Text {
            x: 4.0,
            y: 5.0,
            text: "hello".into(),
            font_size: 12.0,
            weight: 400,
            color: Color::from_hex("#112233").unwrap(),
            max_width: Some(80.0),
            alignment: TextAlignment::Center,
        }]);
        let json = serde_json::to_string(&list).unwrap();
        let decoded: DisplayList = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, list);
    }
}
