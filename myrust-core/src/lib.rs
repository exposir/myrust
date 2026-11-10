//! Core desktop runtime for myrust.
//!
//! The crate provides a single application object, native WebView windows,
//! typed IPC, and a small set of cross-platform desktop APIs.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

/// Application lifecycle and native window management.
pub mod app;
/// Native desktop integrations.
pub mod desktop;
/// Typed inter-process communication between Rust and WebView JavaScript.
pub mod ipc;
/// Release, distribution, signing, and update support.
pub mod release;
/// Native UI layout, widgets, display lists, and optional renderers.
pub mod ui;
/// Wry-based WebView wrapper.
pub mod webview;

pub use app::file_watcher::{
    FileEvent, FileEventDetail, FileEventKind, FileWatcher, WatchHandle, WatchOptions,
};
pub use app::recent_documents::{JumpList, RecentDocument, RecentDocuments, UnsupportedJumpList};
pub use app::window_state::{JsonFileStore, WindowState, WindowStateStore};
pub use app::{
    App, AppContext, Rect, TitleButtons, Vibrancy, WindowConfig, WindowContent, WindowId,
    WindowInfo,
};
pub use desktop::{
    Activation, CustomProtocol, DownloadEvent, DownloadHandler, DownloadPolicy,
    DownloadPolicyHandler, ProtocolHandler, ProtocolResponse, SingleInstance,
};
pub use release::{
    HttpUpdateSource, LocalUpdateSource, PackageConfig, PackageOutput, Packager, SignatureVerifier,
    SigningConfig, SigningPlatform, UnimplementedSignatureVerifier, UpdateInfo, UpdateSource,
    Updater,
};
pub use ui::{
    button, checkbox, column, container, create_renderer, default_font, layout_text,
    resolve_button_regions, row, scroll_view, slider, spacer, text, text_field, AccessNode,
    AlignItems, Animation, ApproximateMeasurer, ButtonState, CheckboxState, Color, ColorParseError,
    ColumnBuilder, CommandCounts, ContainerBuilder, Context, CpuFrame, CpuRasterizer, DisplayList,
    DrawCommand, Easing, Edges, FlexBuilder, FlexDirection, FontFace, FontRegistry, FontSizes,
    GlyphBitmap, GradientStop, Icon, ImageHandle, InputState, InputTranslator, InteractiveRegion,
    JustifyContent, Key, LayoutEngine, LayoutResult, LayoutScope, NativeInput, NativeUi,
    NativeUiError, Node, Point, PointerButton, Position, Radius, RasterError, RealFontMeasurer,
    Rect as UiRect, RenderError, RenderSize, RenderStats, Renderer, RendererBackend, RendererKind,
    Role, RowBuilder, ScrollState, Size, SliderState, SoftwareRenderer, Spacing, Style, SvgPath,
    TextAlignment, TextBuilder, TextFieldState, TextLine, TextMeasurer, TextMetrics, Theme,
    Transform, UiAction, UiEvent, UiEventRouter, ViewFrame,
};
#[cfg(feature = "gpu")]
pub use ui::{GpuInfo, WgpuRenderer};
pub use webview::{
    ContentLoadType, ContentResourceType, ContentRule, ContentRuleAction, ContentRuleTrigger,
    FileDropEvent, FileDropHandler, FileDropKind, FindOptions, MemorySessionStateStore, PageHandle,
    PageOptions, PageSource, ScrollPosition, Session, SessionHistoryEntry, SessionKind,
    SessionState, SessionStateStore, WebViewConfig, WebViewHandle,
};
