//! Renderer abstraction, backend selection, and CPU-only fallback.
//!
//! The [`Renderer`] trait deliberately does not mention wgpu types. The
//! optional `gpu` feature provides a real wgpu offscreen backend, while
//! [`SoftwareRenderer`] validates and analyzes display lists without
//! rasterizing pixels.

use std::fmt;

use super::layout::Point;
use super::paint::{Color, DisplayList, DrawCommand, Rect, Transform};

#[cfg(feature = "gpu")]
mod wgpu_backend;

#[cfg(feature = "gpu")]
pub use wgpu_backend::{GpuInfo, WgpuRenderer};

/// Render-target dimensions in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderSize {
    /// Target width.
    pub width: u32,
    /// Target height.
    pub height: u32,
}

impl RenderSize {
    /// Creates a render-target size.
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Returns whether either dimension is zero.
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Concrete rendering backend selected for a renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererBackend {
    /// CPU-only display-list analysis; it does not produce pixels.
    Software,
    /// Optional wgpu offscreen renderer.
    Wgpu,
}

impl RendererBackend {
    /// Returns a stable backend name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Software => "software",
            Self::Wgpu => "wgpu",
        }
    }
}

impl fmt::Display for RendererBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Backend preference used by [`create_renderer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RendererKind {
    /// Use the CPU fallback without touching GPU APIs.
    Software,
    /// Prefer wgpu and automatically fall back when no usable device exists.
    #[default]
    Auto,
    /// Require wgpu; return an explicit error when it is unavailable.
    Wgpu,
}

impl fmt::Display for RendererKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Software => "software",
            Self::Auto => "auto",
            Self::Wgpu => "wgpu",
        })
    }
}

/// Per-command counters produced by a render pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommandCounts {
    /// Axis-aligned rectangle count.
    pub rect: usize,
    /// Rounded rectangle count.
    pub rounded_rect: usize,
    /// Circle count.
    pub circle: usize,
    /// Line count.
    pub line: usize,
    /// Text count.
    pub text: usize,
    /// SVG path count.
    pub svg_path: usize,
    /// Linear-gradient count.
    pub linear_gradient: usize,
    /// Image count.
    pub image: usize,
    /// Scissor-push state count.
    pub scissor_push: usize,
    /// Scissor-pop state count.
    pub scissor_pop: usize,
    /// Transform-push state count.
    pub transform_push: usize,
    /// Transform-pop state count.
    pub transform_pop: usize,
}

impl CommandCounts {
    /// Returns the total number of display-list commands.
    pub fn total(self) -> usize {
        self.rect
            + self.rounded_rect
            + self.circle
            + self.line
            + self.text
            + self.svg_path
            + self.linear_gradient
            + self.image
            + self.scissor_push
            + self.scissor_pop
            + self.transform_push
            + self.transform_pop
    }

    /// Returns commands that this batch cannot draw yet.
    ///
    /// Text and SVG paths are now drawn by the CPU presenter; images remain
    /// unsupported because the display list carries only a source handle.
    pub fn unsupported(self) -> usize {
        self.image
    }

    pub(crate) fn record(&mut self, command: &DrawCommand) {
        match command {
            DrawCommand::Rect { .. } => self.rect += 1,
            DrawCommand::RoundedRect { .. } => self.rounded_rect += 1,
            DrawCommand::Circle { .. } => self.circle += 1,
            DrawCommand::Line { .. } => self.line += 1,
            DrawCommand::Text { .. } => self.text += 1,
            DrawCommand::SvgPath { .. } => self.svg_path += 1,
            DrawCommand::LinearGradient { .. } => self.linear_gradient += 1,
            DrawCommand::Image { .. } => self.image += 1,
            DrawCommand::ScissorPush { .. } => self.scissor_push += 1,
            DrawCommand::ScissorPop => self.scissor_pop += 1,
            DrawCommand::TransformPush { .. } => self.transform_push += 1,
            DrawCommand::TransformPop => self.transform_pop += 1,
        }
    }
}

/// Observable result of the most recent render call.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RenderStats {
    /// Total commands consumed from the display list.
    pub total_commands: usize,
    /// Counts split by command kind.
    pub command_counts: CommandCounts,
    /// Image commands that were not rendered.
    pub unsupported_commands: usize,
    /// GPU draw calls issued. Always zero for the software backend.
    pub draw_calls: usize,
    /// GPU vertices submitted. Always zero for the software backend.
    pub drawn_vertices: usize,
    /// Deepest transform stack reached.
    pub max_transform_depth: usize,
    /// Deepest scissor stack reached.
    pub max_scissor_depth: usize,
    /// Union of supported geometry after transform and scissor.
    pub effective_bounds: Option<Rect>,
    /// Scissor state used for the most recent supported geometry command.
    pub effective_scissor: Option<Rect>,
}

/// Renderer interface independent of any concrete graphics API.
pub trait Renderer {
    /// Returns the selected backend.
    fn backend(&self) -> RendererBackend;

    /// Returns the current device/render-target size.
    fn device_size(&self) -> RenderSize;

    /// Returns the current presentation/surface size.
    fn surface_size(&self) -> RenderSize;

    /// Resizes the render target.
    fn resize(&mut self, size: RenderSize) -> Result<(), RenderError>;

    /// Renders one display list and returns observable statistics.
    ///
    /// Image commands are not drawable in this batch. Text and SVG paths are
    /// analyzed by the software renderer and drawn by the CPU presenter.
    /// Implementations report them through [`RenderStats::unsupported_commands`]
    /// and the corresponding [`CommandCounts`] fields instead of silently
    /// discarding them.
    fn render(&mut self, display_list: &DisplayList) -> Result<RenderStats, RenderError>;
}

/// Errors produced by renderer initialization or rendering.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// wgpu found no adapter compatible with this environment.
    #[error("wgpu could not find a compatible graphics adapter")]
    NoAdapter,
    /// The requested backend was not compiled or is otherwise unavailable.
    #[error("renderer backend '{backend}' is unavailable: {reason}")]
    BackendUnavailable {
        /// Backend identifier.
        backend: &'static str,
        /// Human-readable reason.
        reason: &'static str,
    },
    /// wgpu could not create a logical device.
    #[error("wgpu device request failed: {message}")]
    DeviceRequest {
        /// Backend error message.
        message: String,
    },
    /// An asynchronous backend operation exceeded its timeout.
    #[error("{operation} timed out after {milliseconds} ms")]
    Timeout {
        /// Operation that timed out.
        operation: &'static str,
        /// Timeout in milliseconds.
        milliseconds: u64,
    },
    /// Render-target dimensions may not be zero.
    #[error("render target size must be non-zero, got {width}x{height}")]
    InvalidSize {
        /// Invalid width.
        width: u32,
        /// Invalid height.
        height: u32,
    },
    /// Draw commands ended a transform or scissor scope that was not open.
    #[error("display list has an unbalanced {command} command")]
    UnbalancedState {
        /// Name of the offending command.
        command: &'static str,
    },
    /// The graphics backend failed after initialization.
    #[error("graphics backend failure: {message}")]
    BackendFailure {
        /// Backend error message.
        message: String,
    },
}

/// CPU-only renderer that validates command state without producing pixels.
///
/// It is a fallback for environments without a graphics device and also a
/// deterministic test double for display-list traversal. It applies transform
/// and scissor state to compute observable geometry bounds, but it performs no
/// rasterization, font shaping, SVG tessellation, or image decoding.
#[derive(Debug, Clone)]
pub struct SoftwareRenderer {
    device_size: RenderSize,
    surface_size: RenderSize,
    last_stats: RenderStats,
}

impl SoftwareRenderer {
    /// Creates a software renderer for the supplied surface size.
    pub fn new(size: RenderSize) -> Self {
        Self {
            device_size: size,
            surface_size: size,
            last_stats: RenderStats::default(),
        }
    }

    /// Returns statistics from the most recent successful render.
    pub fn last_stats(&self) -> &RenderStats {
        &self.last_stats
    }

    /// Returns transformed, clipped bounds from the most recent render.
    pub fn last_bounds(&self) -> Option<Rect> {
        self.last_stats.effective_bounds
    }
}

impl Renderer for SoftwareRenderer {
    fn backend(&self) -> RendererBackend {
        RendererBackend::Software
    }

    fn device_size(&self) -> RenderSize {
        self.device_size
    }

    fn surface_size(&self) -> RenderSize {
        self.surface_size
    }

    fn resize(&mut self, size: RenderSize) -> Result<(), RenderError> {
        validate_size(size)?;
        self.device_size = size;
        self.surface_size = size;
        self.last_stats = RenderStats::default();
        Ok(())
    }

    fn render(&mut self, display_list: &DisplayList) -> Result<RenderStats, RenderError> {
        validate_size(self.surface_size)?;
        let stats = analyze_display_list(display_list)?;
        self.last_stats.clone_from(&stats);
        Ok(stats)
    }
}

/// Creates a renderer using the requested backend preference.
///
/// Existing HTTP, WebView, and App startup paths never call this function
/// implicitly. `Auto` may initialize wgpu only when this API is invoked
/// explicitly and falls back to [`SoftwareRenderer`] if no adapter or device
/// is available.
pub fn create_renderer(
    kind: RendererKind,
    size: RenderSize,
) -> Result<Box<dyn Renderer>, RenderError> {
    validate_size(size)?;
    match kind {
        RendererKind::Software => Ok(Box::new(SoftwareRenderer::new(size))),
        RendererKind::Auto => create_auto_renderer(size),
        RendererKind::Wgpu => create_wgpu_renderer(size),
    }
}

#[cfg(feature = "gpu")]
fn create_auto_renderer(size: RenderSize) -> Result<Box<dyn Renderer>, RenderError> {
    match WgpuRenderer::new(size) {
        Ok(renderer) => Ok(Box::new(renderer)),
        Err(error) if should_fallback(&error) => {
            tracing::warn!(%error, "wgpu unavailable; using software renderer");
            Ok(Box::new(SoftwareRenderer::new(size)))
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(feature = "gpu"))]
fn create_auto_renderer(size: RenderSize) -> Result<Box<dyn Renderer>, RenderError> {
    tracing::debug!("myrust-core built without the gpu feature; using software renderer");
    Ok(Box::new(SoftwareRenderer::new(size)))
}

#[cfg(feature = "gpu")]
fn create_wgpu_renderer(size: RenderSize) -> Result<Box<dyn Renderer>, RenderError> {
    WgpuRenderer::new(size).map(|renderer| Box::new(renderer) as Box<dyn Renderer>)
}

#[cfg(not(feature = "gpu"))]
fn create_wgpu_renderer(_size: RenderSize) -> Result<Box<dyn Renderer>, RenderError> {
    Err(RenderError::BackendUnavailable {
        backend: "wgpu",
        reason: "myrust-core was compiled without the gpu feature",
    })
}

#[cfg(any(feature = "gpu", test))]
fn should_fallback(error: &RenderError) -> bool {
    matches!(
        error,
        RenderError::NoAdapter
            | RenderError::BackendUnavailable { .. }
            | RenderError::DeviceRequest { .. }
            | RenderError::Timeout { .. }
            | RenderError::BackendFailure { .. }
    )
}

fn validate_size(size: RenderSize) -> Result<(), RenderError> {
    if size.is_empty() {
        Err(RenderError::InvalidSize {
            width: size.width,
            height: size.height,
        })
    } else {
        Ok(())
    }
}

fn analyze_display_list(display_list: &DisplayList) -> Result<RenderStats, RenderError> {
    let mut stats = RenderStats {
        total_commands: display_list.len(),
        ..RenderStats::default()
    };
    let mut transform = TransformMatrix::IDENTITY;
    let mut transform_stack = Vec::new();
    let mut scissor = None;
    let mut scissor_stack = Vec::new();

    for command in &display_list.commands {
        stats.command_counts.record(command);
        match command {
            DrawCommand::TransformPush {
                transform: local, ..
            } => {
                transform_stack.push(transform);
                transform = transform.then(TransformMatrix::from_transform(*local));
                stats.max_transform_depth = stats.max_transform_depth.max(transform_stack.len());
            }
            DrawCommand::TransformPop => {
                transform = transform_stack.pop().ok_or(RenderError::UnbalancedState {
                    command: "TransformPop",
                })?;
            }
            DrawCommand::ScissorPush { rect } => {
                scissor_stack.push(scissor);
                let transformed = transform.transform_rect(*rect);
                scissor = Some(match scissor {
                    Some(current) => intersect_rects(current, transformed),
                    None => transformed,
                });
                stats.max_scissor_depth = stats.max_scissor_depth.max(scissor_stack.len());
            }
            DrawCommand::ScissorPop => {
                scissor = scissor_stack.pop().ok_or(RenderError::UnbalancedState {
                    command: "ScissorPop",
                })?;
            }
            DrawCommand::Text { .. } | DrawCommand::SvgPath { .. } | DrawCommand::Image { .. } => {}
            _ => {
                if let Some(bounds) = command_bounds(command) {
                    let transformed = transform.transform_rect(bounds);
                    stats.effective_scissor = scissor;
                    if let Some(clipped) = clip_bounds(transformed, scissor) {
                        include_bounds(&mut stats.effective_bounds, clipped);
                    }
                }
            }
        }
    }

    stats.unsupported_commands = stats.command_counts.unsupported();
    Ok(stats)
}

fn command_bounds(command: &DrawCommand) -> Option<Rect> {
    match command {
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
        | DrawCommand::LinearGradient {
            x,
            y,
            width,
            height,
            ..
        } => Some(Rect::new(*x, *y, *width, *height)),
        DrawCommand::Circle { x, y, radius, .. } => Some(Rect::new(
            *x - *radius,
            *y - *radius,
            *radius * 2.0,
            *radius * 2.0,
        )),
        DrawCommand::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            ..
        } => {
            let half_width = width.max(0.0) / 2.0;
            Some(Rect::new(
                x1.min(*x2) - half_width,
                y1.min(*y2) - half_width,
                (x1 - x2).abs() + half_width * 2.0,
                (y1 - y2).abs() + half_width * 2.0,
            ))
        }
        DrawCommand::Text { .. }
        | DrawCommand::SvgPath { .. }
        | DrawCommand::Image { .. }
        | DrawCommand::ScissorPush { .. }
        | DrawCommand::ScissorPop
        | DrawCommand::TransformPush { .. }
        | DrawCommand::TransformPop => None,
    }
}

fn include_bounds(bounds: &mut Option<Rect>, rect: Rect) {
    if !rect.is_valid() || rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    *bounds = Some(match *bounds {
        Some(current) => current.union(rect),
        None => rect,
    });
}

fn clip_bounds(bounds: Rect, scissor: Option<Rect>) -> Option<Rect> {
    let clipped = match scissor {
        Some(scissor) => bounds.intersection(scissor)?,
        None => bounds,
    };
    (clipped.is_valid() && clipped.width > 0.0 && clipped.height > 0.0).then_some(clipped)
}

pub(crate) fn intersect_rects(current: Rect, next: Rect) -> Rect {
    current
        .intersection(next)
        .unwrap_or_else(|| Rect::new(current.x.max(next.x), current.y.max(next.y), 0.0, 0.0))
}

/// Affine matrix shared by CPU analysis and the optional wgpu backend.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TransformMatrix {
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
    tx: f32,
    ty: f32,
}

impl TransformMatrix {
    pub(crate) const IDENTITY: Self = Self {
        m11: 1.0,
        m12: 0.0,
        m21: 0.0,
        m22: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub(crate) fn from_transform(transform: Transform) -> Self {
        let cos = transform.rotation.cos();
        let sin = transform.rotation.sin();
        Self {
            m11: cos * transform.scale.x,
            m12: -sin * transform.scale.y,
            m21: sin * transform.scale.x,
            m22: cos * transform.scale.y,
            tx: transform.translation.x,
            ty: transform.translation.y,
        }
    }

    pub(crate) fn apply(self, point: Point) -> Point {
        Point::new(
            self.m11 * point.x + self.m12 * point.y + self.tx,
            self.m21 * point.x + self.m22 * point.y + self.ty,
        )
    }

    pub(crate) fn then(self, child: Self) -> Self {
        Self {
            m11: self.m11 * child.m11 + self.m12 * child.m21,
            m12: self.m11 * child.m12 + self.m12 * child.m22,
            m21: self.m21 * child.m11 + self.m22 * child.m21,
            m22: self.m21 * child.m12 + self.m22 * child.m22,
            tx: self.m11 * child.tx + self.m12 * child.ty + self.tx,
            ty: self.m21 * child.tx + self.m22 * child.ty + self.ty,
        }
    }

    pub(crate) fn scale_factor(self) -> f32 {
        let x = (self.m11 * self.m11 + self.m21 * self.m21).sqrt();
        let y = (self.m12 * self.m12 + self.m22 * self.m22).sqrt();
        (x + y) / 2.0
    }

    pub(crate) fn transform_rect(self, rect: Rect) -> Rect {
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

pub(crate) fn color_to_array(color: Color) -> [f32; 4] {
    [
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        f32::from(color.a) / 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::paint::{Color, GradientStop, ImageHandle, Radius, SvgPath, TextAlignment};

    fn test_size() -> RenderSize {
        RenderSize::new(320, 200)
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> DrawCommand {
        DrawCommand::Rect {
            x,
            y,
            width,
            height,
            color: Color::WHITE,
        }
    }

    #[test]
    fn software_renderer_counts_every_command_kind() {
        let display = DisplayList::from_commands(vec![
            rect(0.0, 0.0, 10.0, 10.0),
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                radius: Radius::all(2.0),
                color: Color::WHITE,
            },
            DrawCommand::Circle {
                x: 5.0,
                y: 5.0,
                radius: 3.0,
                color: Color::WHITE,
            },
            DrawCommand::Line {
                x1: 0.0,
                y1: 0.0,
                x2: 10.0,
                y2: 10.0,
                width: 2.0,
                color: Color::WHITE,
            },
            DrawCommand::Text {
                x: 0.0,
                y: 0.0,
                text: "text".into(),
                font_size: 14.0,
                weight: 400,
                color: Color::WHITE,
                max_width: None,
                alignment: TextAlignment::Start,
            },
            DrawCommand::SvgPath {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                path: "M0 0L10 10".into(),
                fill: None,
                stroke: Some(Color::WHITE),
                stroke_width: 1.0,
            },
            DrawCommand::LinearGradient {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                start: Point::new(0.0, 0.0),
                end: Point::new(1.0, 0.0),
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: Color::BLACK,
                    },
                    GradientStop {
                        offset: 1.0,
                        color: Color::WHITE,
                    },
                ],
            },
            DrawCommand::Image {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                source: ImageHandle::new("test-image"),
            },
            DrawCommand::ScissorPush {
                rect: Rect::new(0.0, 0.0, 50.0, 50.0),
            },
            DrawCommand::ScissorPop,
            DrawCommand::TransformPush {
                transform: Transform::identity(),
            },
            DrawCommand::TransformPop,
        ]);
        let mut renderer = SoftwareRenderer::new(test_size());
        let stats = renderer.render(&display).unwrap();
        assert_eq!(stats.total_commands, 12);
        assert_eq!(stats.command_counts.total(), 12);
        assert_eq!(stats.command_counts.text, 1);
        assert_eq!(stats.command_counts.svg_path, 1);
        assert_eq!(stats.command_counts.image, 1);
        assert_eq!(stats.unsupported_commands, 1);
        assert_eq!(stats.draw_calls, 0);
        assert_eq!(stats.drawn_vertices, 0);
    }

    #[test]
    fn software_renderer_applies_transform_to_bounds() {
        let display = DisplayList::from_commands(vec![
            DrawCommand::TransformPush {
                transform: Transform {
                    translation: Point::new(70.0, 35.0),
                    scale: Point::new(2.0, 2.0),
                    rotation: 0.0,
                },
            },
            rect(10.0, 10.0, 20.0, 10.0),
            DrawCommand::TransformPop,
        ]);
        let mut renderer = SoftwareRenderer::new(test_size());
        let stats = renderer.render(&display).unwrap();
        assert_eq!(
            stats.effective_bounds,
            Some(Rect::new(90.0, 55.0, 40.0, 20.0))
        );
        assert_eq!(stats.max_transform_depth, 1);
        assert_eq!(renderer.last_bounds(), stats.effective_bounds);
    }

    #[test]
    fn software_renderer_applies_and_restores_scissor_state() {
        let display = DisplayList::from_commands(vec![
            DrawCommand::ScissorPush {
                rect: Rect::new(20.0, 20.0, 20.0, 20.0),
            },
            rect(10.0, 10.0, 40.0, 40.0),
            DrawCommand::ScissorPop,
            rect(100.0, 100.0, 10.0, 10.0),
        ]);
        let mut renderer = SoftwareRenderer::new(test_size());
        let stats = renderer.render(&display).unwrap();
        assert_eq!(
            stats.effective_bounds,
            Some(Rect::new(20.0, 20.0, 90.0, 90.0))
        );
        assert_eq!(stats.max_scissor_depth, 1);
    }

    #[test]
    fn software_renderer_rejects_unbalanced_state() {
        let display = DisplayList::from_commands(vec![DrawCommand::TransformPop]);
        let mut renderer = SoftwareRenderer::new(test_size());
        assert_eq!(
            renderer.render(&display).unwrap_err(),
            RenderError::UnbalancedState {
                command: "TransformPop"
            }
        );
    }

    #[test]
    fn renderer_trait_object_exposes_metadata_and_stats() {
        let mut renderer: Box<dyn Renderer> =
            Box::new(SoftwareRenderer::new(RenderSize::new(64, 32)));
        assert_eq!(renderer.backend(), RendererBackend::Software);
        assert_eq!(renderer.device_size(), RenderSize::new(64, 32));
        assert_eq!(renderer.surface_size(), RenderSize::new(64, 32));
        let stats = renderer
            .render(&DisplayList::from_commands(vec![rect(0.0, 0.0, 8.0, 8.0)]))
            .unwrap();
        assert_eq!(stats.command_counts.rect, 1);
    }

    #[test]
    fn software_resize_validates_and_updates_both_sizes() {
        let mut renderer = SoftwareRenderer::new(test_size());
        renderer.resize(RenderSize::new(10, 10)).unwrap();
        assert_eq!(renderer.device_size(), RenderSize::new(10, 10));
        assert_eq!(renderer.surface_size(), RenderSize::new(10, 10));
        assert_eq!(
            renderer.resize(RenderSize::new(0, 10)).unwrap_err(),
            RenderError::InvalidSize {
                width: 0,
                height: 10
            }
        );
    }

    #[test]
    fn no_adapter_error_is_explicit_and_fallbackable() {
        let error = RenderError::NoAdapter;
        assert!(error.to_string().contains("adapter"));
        assert!(should_fallback(&error));
        assert!(!should_fallback(&RenderError::InvalidSize {
            width: 0,
            height: 1
        }));
    }

    #[cfg(not(feature = "gpu"))]
    #[test]
    fn requiring_wgpu_without_feature_returns_explicit_error() {
        let error = create_renderer(RendererKind::Wgpu, test_size())
            .err()
            .expect("wgpu must be unavailable without its feature");
        assert!(matches!(
            error,
            RenderError::BackendUnavailable {
                backend: "wgpu",
                ..
            }
        ));
    }

    #[test]
    fn explicit_software_selection_never_touches_gpu() {
        let renderer = create_renderer(RendererKind::Software, test_size()).unwrap();
        assert_eq!(renderer.backend(), RendererBackend::Software);
    }

    #[cfg(not(feature = "gpu"))]
    #[test]
    fn auto_falls_back_to_software_without_gpu_feature() {
        let renderer = create_renderer(RendererKind::Auto, test_size()).unwrap();
        assert_eq!(renderer.backend(), RendererBackend::Software);
    }

    #[test]
    fn svg_path_shape_preserves_serializable_payload() {
        let path = SvgPath::stroked("M0 0L1 1", 1.0, Color::WHITE);
        let encoded = serde_json::to_string(&path).unwrap();
        let decoded: SvgPath = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, path);
    }
}
