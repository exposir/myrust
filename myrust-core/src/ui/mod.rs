//! Native UI layout, widgets, painting, text, icons, and renderer backends.
//!
//! The default build remains pure CPU. The optional `gpu` feature adds a
//! wgpu offscreen renderer for supported geometry; [`SoftwareRenderer`]
//! remains available as a non-rasterizing fallback for every build.
//! [`NativeUi`] implements the immediate-mode `View(callback)` paradigm:
//! widgets are constructed directly on [`Context`] each frame, query input
//! inline, and produce a fresh display list. Hover, Tab focus traversal, and
//! text-field/checkbox/slider/scroll-view controls are built on that same
//! per-frame model. [`CpuRasterizer`] provides the optional software
//! presentation path used by the native Dashboard demo.

/// Platform-neutral accessibility semantics.
pub mod access;
/// Animation timing and easing primitives.
pub mod animation;
/// Immediate-mode UI context and widget builders.
pub mod context;
/// Platform-neutral native UI input events and routing.
pub mod event;
/// System TTF loading, glyph rasterization, and real-font measurement.
pub mod font;
/// Built-in SVG path icons.
pub mod icon;
/// Deterministic CPU layout solver.
pub mod layout;
/// Event-driven native window content runtime.
pub mod native;
/// Serializable drawing instructions and display-list geometry.
pub mod paint;
/// CPU bitmap presentation for native windows without a GPU.
pub mod present;
/// Renderer abstraction, backend selection, and CPU fallback.
pub mod render;
/// Deterministic approximate text measurement and wrapping.
pub mod text;
/// Theme, spacing, and typography tokens.
pub mod theme;
/// Immediate-mode widget builder re-exports.
pub mod widgets;

pub use access::{AccessNode, Role};
pub use animation::{Animation, Easing};
pub use context::{
    button, checkbox, column, container, row, scroll_view, slider, spacer, text, text_field,
    ButtonState, CheckboxState, ColumnBuilder, ContainerBuilder, Context, FlexBuilder, LayoutScope,
    RowBuilder, ScrollState, SliderState, TextBuilder, TextFieldState,
};
pub use event::{
    resolve_button_regions, InputState, InputTranslator, InteractiveRegion, Key, NativeInput,
    PointerButton, UiAction, UiEvent, UiEventRouter,
};
pub use font::{default_font, FontFace, FontRegistry, GlyphBitmap, RealFontMeasurer};
pub use icon::Icon;
pub use layout::{
    AlignItems, Edges, FlexDirection, JustifyContent, LayoutEngine, LayoutResult, Node, Point,
    Position, Size, Style,
};
pub use native::{NativeUi, NativeUiError, ViewFrame};
pub use paint::{
    Color, ColorParseError, DisplayList, DrawCommand, GradientStop, ImageHandle, Radius, Rect,
    SvgPath, TextAlignment, Transform,
};
pub use present::{CpuFrame, CpuRasterizer, RasterError};
pub use render::{
    create_renderer, CommandCounts, RenderError, RenderSize, RenderStats, Renderer,
    RendererBackend, RendererKind, SoftwareRenderer,
};
#[cfg(feature = "gpu")]
pub use render::{GpuInfo, WgpuRenderer};
pub use text::{layout_text, ApproximateMeasurer, TextLine, TextMeasurer, TextMetrics};
pub use theme::{FontSizes, Spacing, Theme};
