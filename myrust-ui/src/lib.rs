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
/// Expandable accordion sections.
pub mod accordion;
/// Animation timing and easing primitives.
pub mod animation;
/// Text input with floating suggestions.
pub mod autocomplete;
/// Initials/image avatar.
pub mod avatar;
/// Clickable path breadcrumbs.
pub mod breadcrumbs;
/// Month calendar.
pub mod calendar;
/// Minimal bar/line charts.
pub mod charts;
/// Read-only code viewer.
pub mod code_editor;
/// Color swatch with palette popup.
pub mod color_picker;
/// Editable combobox.
pub mod combobox;
/// Immediate-mode UI context and widget builders.
pub mod context;
/// Batch 1 controls: switch, radio group, select, tabs, progress, spinner,
/// badge, and collapsible.
pub mod controls;
/// ISO date input with calendar popup.
pub mod date_input;
/// Modal dialogs and alert dialogs.
pub mod dialog;
/// Line diff viewer.
pub mod diff;
/// Layout direction.
pub mod direction;
/// In-window drag and drop.
pub mod drag;
/// In-place editable label.
pub mod editable_text;
/// Platform-neutral native UI input events and routing.
pub mod event;
/// Find bar with match navigation.
pub mod find_bar;
/// Batch 2a anchored surfaces: menus, tooltips, and popovers.
pub mod floating;
/// System TTF loading, glyph rasterization, and real-font measurement.
pub mod font;
/// Form field layout and accessible label grouping.
pub mod form;
/// Scrollable fixed-size grid view.
pub mod grid_view;
/// Built-in SVG path icons.
pub mod icon;
/// RGBA image drawing contract.
pub mod image;
/// Debug inspector for a previous frame.
pub mod inspector;
/// Minimal en-US/zh-CN formatting.
pub mod languages;
/// Deterministic CPU layout solver.
pub mod layout;
/// Clickable text link.
pub mod link;
/// Virtualized vertical list.
pub mod list;
/// Minimal Markdown parser and renderer.
pub mod markdown;
/// Three-tone quantity meter.
pub mod meter;
/// Control-id state migration helper.
pub mod migration;
/// Event-driven native window content runtime.
pub mod native;
/// Numeric and fixed-point text-field wrappers.
pub mod numeric;
/// Anchored overlay/popup primitive with z-order and dismiss semantics.
pub mod overlay;
/// Serializable drawing instructions and display-list geometry.
pub mod paint;
/// Previous-frame timing statistics.
pub mod performance;
/// CPU bitmap presentation for native windows without a GPU.
pub mod present;
/// Dual-knob range slider.
pub mod range_slider;
/// Star rating control.
pub mod rating;
/// Renderer abstraction, backend selection, and CPU fallback.
pub mod render;
/// Read-only formatted text.
pub mod rich_text;
/// UI page router and history buttons.
pub mod router;
/// Single-line search field.
pub mod search_field;
/// Horizontal segmented control.
pub mod segmented;
/// Grouped, single-selection source list.
pub mod sidebar;
/// Two-panel split view with a draggable divider.
pub mod split;
/// Integer stepper.
pub mod stepper;
/// SVG path subset parser.
pub mod svg;
/// Data table with sortable headers and row selection.
pub mod table;
/// Deterministic approximate text measurement and wrapping.
pub mod text;
/// Text-input state/geometry primitives.
pub mod text_input;
/// Theme, spacing, and typography tokens.
pub mod theme;
/// Monotonic clock used by frame timing.
pub mod time;
/// HH:MM time input.
pub mod time_input;
/// Non-modal, stacking toast notifications.
pub mod toast;
/// Pressed-state toggle button.
pub mod toggle;
/// Chip-style token field.
pub mod token_field;
/// Horizontal toolbar with overflow.
pub mod toolbar;
/// Bounds interpolation for layout transitions.
pub mod transitions;
/// Expandable tree and outline rows.
pub mod tree;
/// Immediate-mode widget builder re-exports.
pub mod widgets;

pub use access::{AccessNode, Role};
pub use accordion::{
    accordion, AccordionMode, AccordionResponse, AccordionSection, AccordionState,
};
pub use animation::{Animation, Easing};
pub use time::{Duration, Instant};
pub use autocomplete::{autocomplete, AutocompleteResponse, AutocompleteState};
pub use avatar::{avatar, avatar_image, AvatarShape, AvatarState};
pub use breadcrumbs::{breadcrumbs, BreadcrumbItem, BreadcrumbsResponse};
pub use calendar::{calendar, CalendarResponse, CalendarState};
pub use charts::{bar_chart, line_chart, ChartKind, ChartResponse};
pub use code_editor::{
    code_editor, tokenize_rust_line, CodeEditorResponse, CodeEditorState, CodeToken, CodeTokenKind,
};
pub use color_picker::{color_picker, ColorPickerResponse, ColorPickerState, PRESET_COLORS};
pub use combobox::{combobox, ComboboxResponse, ComboboxState};
pub use context::{
    button, button_primary, button_primary_with_icon, button_with_icon, checkbox, checkbox_labeled, checkbox_indeterminate, column, container, grid, icon_button, row, scroll_view,
    slider, spacer, text, text_area, text_field, text_field_password, transform, ButtonState, CheckboxState, ColumnBuilder,
    ContainerBuilder, Context, FlexBuilder, GridBuilder, LayoutScope, RowBuilder, ScrollState,
    SliderState, TextBuilder, TextFieldState,
};
pub use controls::{
    badge, collapsible, progress, progress_indeterminate, radio_group, select, spinner, switch,
    tabs, BadgeState, CollapsibleState, ProgressState, RadioGroupState, SelectState, SpinnerState,
    SwitchState, TabsState,
};
pub use date_input::{date_input, DateInputResponse, DateInputState};
pub use dialog::{alert_dialog, dialog, DialogAction, DialogResponse};
pub use diff::{
    diff_lines, diff_viewer, DiffKind, DiffLayout, DiffLine, DiffViewerResponse, DiffViewerState,
};
pub use direction::Direction;
pub use drag::{
    drag_source, drop_target, reorder, reorder_list, DragDrop, DragSourceResponse, DragState,
    DropTargetResponse, ListReorderResponse, ListReorderState,
};
pub use editable_text::{editable_text, EditableTextResponse, EditableTextState};
pub use event::{
    resolve_button_regions, InputState, InputTranslator, InteractiveRegion, Key, NativeInput,
    PointerButton, UiAction, UiEvent, UiEventRouter,
};
pub use find_bar::{count_matches, find_bar, FindBarState};
pub use floating::{
    context_menu, menu_button, popover, tooltip, tooltip_for, tooltip_with_delay,
    ContextMenuResponse, ContextMenuState, MenuButtonState, MenuState, TooltipState, TOOLTIP_DELAY,
};
pub use font::{
    default_font, install_default_font, FontFace, FontRegistry, GlyphBitmap, RealFontMeasurer,
};
pub use form::{form_field, FormFieldState};
pub use grid_view::{grid_view, GridViewResponse, GridViewState};
pub use icon::{icon, Icon, IconState};
pub use image::{image, ImageData, ImageState};
pub use inspector::{
    inspect_display, inspect_frame, inspector, InspectorItem, InspectorResponse, InspectorTree,
    PaintSummary,
};
pub use languages::{format_date, format_number, DateStyle, FormatError, Locale};
pub use layout::{
    AlignItems, Edges, FlexDirection, JustifyContent, LayoutEngine, LayoutResult, Node, Point,
    Position, Size, Style,
};
pub use link::{link, LinkState};
pub use list::{list, ListResponse, ListRow, ListState};
pub use markdown::{
    markdown, parse_inline, parse_markdown, InlineSpan, InlineStyle, MarkdownBlock,
    MarkdownResponse,
};
pub use meter::{meter, MeterState, MeterTone};
pub use migration::migrate_value;
pub use native::{NativeUi, NativeUiError, ViewFrame};
pub use numeric::{
    decimal_input, format_decimal_units, number_input, parse_decimal_units, DecimalInputOptions,
    DecimalInputState, NumberInputOptions, NumberInputState,
};
pub use overlay::{overlay, popup, OverlayResponse, OverlayState};
pub use paint::{
    Color, ColorParseError, DisplayList, DrawCommand, GradientStop, ImageHandle, Radius, Rect,
    SvgPath, TextAlignment, Transform,
};
pub use performance::{frame_stats_panel, FrameStats, FrameStatsResponse};
pub use present::{CpuFrame, CpuRasterizer, RasterError, TextRenderCommand};
pub use range_slider::{range_slider, RangeKnob, RangeSliderResponse, RangeSliderState};
pub use rating::{rating, RatingState};
pub use render::{
    create_renderer, CommandCounts, RenderError, RenderSize, RenderStats, Renderer,
    RendererBackend, RendererKind, SoftwareRenderer,
};
#[cfg(feature = "gpu")]
pub use render::{GpuInfo, WgpuRenderer};
pub use rich_text::{rich_text, RichTextResponse};
pub use router::{
    back_forward_buttons, router, BackForwardState, Page, RouterResponse, RouterState,
};
pub use search_field::{search_field, SearchFieldState};
pub use segmented::{segmented, SegmentedResponse};
pub use sidebar::{sidebar, SidebarItem, SidebarResponse, SidebarSection};
pub use split::{
    split_view, SplitOrientation, SplitViewResponse, SplitViewState, MAX_SPLIT_RATIO,
    MIN_SPLIT_RATIO,
};
pub use stepper::{stepper, StepperState};
pub use svg::{parse_svg_path, SvgContour, SvgOutline};
pub use table::{
    table, SortDirection, TableColumn, TableResponse, TableRow, TableSort, TableState,
};
pub use text::{layout_text, ApproximateMeasurer, TextLine, TextMeasurer, TextMetrics};
pub use text_input::TextInputState;
pub use theme::{FontSizes, Spacing, Theme};
pub use time_input::{time_input, TimeInputState};
pub use toast::{
    toasts, Toast, ToastDismissal, ToastEvent, ToastQueue, ToastResponse, TOAST_DURATION,
};
pub use toggle::{toggle, ToggleGroup, ToggleState};
pub use token_field::{token_field, TokenFieldState};
pub use toolbar::{overflow_split, toolbar, ToolbarItem, ToolbarResponse, ToolbarState};
pub use transitions::{instant_state, transition, TransitionFrame, TransitionState};
pub use tree::{outline, tree, TreeNode, TreeResponse, TreeState};
