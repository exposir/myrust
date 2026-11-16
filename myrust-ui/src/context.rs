//! Immediate-mode UI context and widget builders.
//!
//! Widgets are constructed directly on [`Context`] during every frame. Layout
//! nodes and drawing commands are collected first, then resolved into absolute
//! coordinates after the user callback returns.
//!
//! Focus order follows widget construction order. Explicit ids must be unique;
//! duplicate ids share interaction, focus, and persistent widget state.

use std::collections::HashMap;

use super::time::{Duration, Instant};

use super::access::{AccessNode, Role};
use super::accordion::{AccordionResponse, AccordionSection, AccordionState};
use super::animation::{Animation, Easing};
use super::autocomplete::{filter_suggestions, AutocompleteResponse, AutocompleteState};
use super::avatar::{AvatarShape, AvatarState};
use super::breadcrumbs::{BreadcrumbItem, BreadcrumbsResponse};
use super::calendar::{
    civil_from_days, days_from_civil, days_in_month, move_month, weekday_monday, CalendarResponse,
    CalendarState,
};
use super::charts::{ChartKind, ChartResponse};
use super::code_editor::{CodeEditorOptions, CodeEditorResponse, CodeEditorState, CodeSeverity};
use super::color_picker::{
    hex_of, ColorPanelResponse, ColorPanelState, ColorPickerResponse, ColorPickerState, Hsv,
    PanelDrag, SWATCH_COLORS, SWATCH_NAMES, PRESET_COLORS,
};
use super::combobox::{filter_options, ComboboxResponse, ComboboxState};
use super::controls::{
    BadgeState, CollapsibleState, ProgressState, RadioGroupState, SelectState, SpinnerState,
    SwitchState, TabsState,
};
use super::date_input::{parse_iso_date, DateInputResponse, DateInputState};
use super::dialog::{DialogAction, DialogResponse};
use super::diff::{
    diff_lines, lines_diff, split_lines, split_rows, word_ranges, DiffLayout, DiffViewerResponse,
    DiffViewerState,
};
use super::direction::Direction;
use super::drag::{
    DragDrop, DragSourceResponse, DragState, DropTargetResponse, ListReorderResponse,
    ListReorderState,
};
use super::editable_text::{EditableTextResponse, EditableTextState};
use super::event::{resolve_button_regions, InteractiveRegion, Key, UiEvent};
use super::find_bar::FindBarState;
use super::floating::{
    ContextMenuResponse, ContextMenuState, MenuButtonState, MenuState, TooltipState,
};
use super::font::{default_font, FontFace};
use super::form::FormFieldState;
use super::grid_view::{GridViewResponse, GridViewState};
use super::icon::{Icon, IconState};
use super::image::{ImageData, ImageState};
use super::inspector::{InspectorItem, InspectorResponse, InspectorTree};
use super::languages::{format_date, DateStyle, Locale};
use super::layout::{
    AlignItems, Edges, FlexDirection, JustifyContent, LayoutEngine, LayoutResult, Node, Point,
    Position, Size, Style,
};
use super::link::LinkState;
use super::list::{ListResponse, ListRow, ListState};
use super::markdown::{
    parse_inline, parse_markdown, InlineSpan, InlineStyle, MarkdownBlock, MarkdownResponse,
    TableAlign,
};
use super::meter::{MeterState, MeterTone};
use super::numeric::{
    filter_numeric_text, parse_decimal_units, DecimalInputOptions, DecimalInputState,
    NumberInputOptions, NumberInputState,
};
use super::overlay::{OverlayResponse, OverlayState};
use super::paint::{
    Color, DisplayList, DrawCommand, GradientStop, Radius, Rect, TextAlignment, Transform,
};
use super::performance::{FrameStats, FrameStatsResponse};
use super::range_slider::{RangeKnob, RangeSliderResponse, RangeSliderState};
use super::rating::RatingState;
use super::render::{CommandCounts, RenderStats};
use super::rich_text::{RichTextResponse, RichTextSpan};
use super::router::{BackForwardState, Page, RouterResponse, RouterState};
use super::search_field::SearchFieldState;
use super::segmented::SegmentedResponse;
use super::sidebar::{SidebarResponse, SidebarSection};
use super::split::{
    SplitOrientation, SplitViewResponse, SplitViewState, MAX_SPLIT_RATIO, MIN_SPLIT_RATIO,
};
use super::stepper::StepperState;
use super::table::{SortDirection, TableColumn, TableResponse, TableRow, TableSort, TableState};
use super::text::{
    host_text_ascent, host_text_descent, host_text_width, host_text_width_family,
    host_text_width_weighted, host_text_x_height, layout_text, ApproximateMeasurer, TextMeasurer,
    TextMetrics,
};
use super::text_input::TextInputState;
use super::theme::Theme;
use super::time_input::{filter_time_text, format_time, parse_time, TimeInputState};
use super::toast::{ToastDismissal, ToastEvent, ToastQueue, ToastResponse};
use super::toggle::{ToggleGroup, ToggleState};
use super::token_field::TokenFieldState;
use super::toolbar::{overflow_split, ToolbarItem, ToolbarResponse, ToolbarState};
use super::tree::{TreeNode, TreeResponse, TreeState};

/// Logical clipping and scroll scope currently active in a frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LayoutScope {
    /// Horizontal scroll offset.
    pub scroll_x: f32,
    /// Vertical scroll offset.
    pub scroll_y: f32,
    /// Optional logical clip rectangle.
    pub clip: Option<Rect>,
}

/// Input snapshot visible to the current immediate-mode frame.
#[derive(Debug, Clone, PartialEq, Default)]
struct InputFrame {
    pointer: Option<Point>,
    pointer_down: bool,
    released: Option<Point>,
    secondary_pressed: Option<Point>,
    wheel: (f32, f32),
    text: String,
    keys: Vec<Key>,
}

enum WidgetState {
    TextField {
        cursor: usize,
        anchor: Option<usize>,
        cursor_x: f32,
        cursor_y: f32,
    },
    Checkbox,
    Slider {
        dragging: bool,
    },
    ScrollView {
        offset: f32,
        dragging: bool,
    },
    Select {
        open: bool,
        highlight: usize,
    },
    Tooltip {
        hovered_since: Option<Instant>,
    },
    SplitView {
        dragging: bool,
    },
    /// A split whose first pane keeps a DIP size, as MyGo's `ui.Split`.
    SplitSized {
        dragging: bool,
        /// The size the pane shows, as last laid out.
        shown: f32,
    },
    RangeSlider {
        dragging: Option<RangeKnob>,
    },
}

struct AnimatedValue {
    from: f32,
    current: f32,
    target: f32,
    started_at: Instant,
    active: bool,
    last_seen_frame: u64,
}

struct PendingAccessNode {
    node_id: u64,
    role: Role,
    label: String,
    value: Option<String>,
    focused: bool,
}

/// Bounds of one overlay root in the frame that produced it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct OverlayFrame {
    layer: u32,
    bounds: Rect,
}

/// Focus bookkeeping that survives between frames for one overlay id.
#[derive(Debug, Clone, Default)]
struct OverlayRuntime {
    was_open: bool,
    restore_focus: Option<String>,
}

/// Per-frame immediate-mode context.
pub struct Context {
    available_size: Size,
    input: InputFrame,
    next_id: u64,
    nodes: HashMap<u64, PendingNode>,
    roots: Vec<u64>,
    parent_stack: Vec<u64>,
    explicit_id: Option<String>,
    scope: LayoutScope,
    pending_interactions: Vec<(u64, String)>,
    previous_regions: HashMap<String, InteractiveRegion>,
    current_regions: HashMap<String, InteractiveRegion>,
    focused_id: Option<String>,
    /// Whether focus came from the keyboard, as MyGo's `focusVisible`:
    /// a pointer press focuses what it clicks without drawing a ring.
    focus_visible: bool,
    focus_order: Vec<String>,
    pending_focusables: Vec<(u64, String, u32)>,
    current_focus_key: Option<String>,
    widget_states: HashMap<String, WidgetState>,
    theme: Theme,
    animations: HashMap<String, AnimatedValue>,
    frame_time: Instant,
    clock_offset: Duration,
    frame_time_override: Option<Instant>,
    frame_serial: u64,
    reduce_motion: bool,
    pending_access_nodes: Vec<PendingAccessNode>,
    current_layer: u32,
    current_transform: Transform,
    current_interaction_transforms: HashMap<String, Transform>,
    previous_interaction_transforms: HashMap<String, Transform>,
    next_overlay_layer: u32,
    frame_overlays: Vec<(u64, String, u32)>,
    previous_overlays: HashMap<String, OverlayFrame>,
    current_overlays: HashMap<String, OverlayFrame>,
    current_layers: HashMap<String, u32>,
    overlay_runtimes: HashMap<String, OverlayRuntime>,
    pending_focus_layer: Option<u32>,
    requested_animation: bool,
    last_frame_stats: FrameStats,
    direction: Direction,
    embedded_overlay: bool,
    widgets_disabled: bool,
    /// Width of the label column of the form being built, so that every
    /// field's label lines up as MyGo's `alignLabels` makes them.
    form_label_width: Option<f32>,
    disabled_theme_backup: Option<Theme>,
    /// Device pixels per logical pixel, as the host draws them, where the
    /// painter snaps a picture's box (`ui/paint.go` snap).
    pixel_scale: f32,
}

impl Context {
    pub(crate) fn new(size: Size) -> Self {
        Self {
            available_size: normalize_size(size),
            input: InputFrame::default(),
            next_id: 1,
            nodes: HashMap::new(),
            roots: Vec::new(),
            parent_stack: Vec::new(),
            explicit_id: None,
            scope: LayoutScope::default(),
            pending_interactions: Vec::new(),
            previous_regions: HashMap::new(),
            current_regions: HashMap::new(),
            focused_id: None,
            focus_visible: false,
            focus_order: Vec::new(),
            pending_focusables: Vec::new(),
            current_focus_key: None,
            widget_states: HashMap::new(),
            theme: Theme::dark(),
            animations: HashMap::new(),
            frame_time: Instant::now(),
            clock_offset: Duration::ZERO,
            frame_time_override: None,
            frame_serial: 0,
            reduce_motion: detect_reduce_motion(),
            pending_access_nodes: Vec::new(),
            current_layer: 0,
            current_transform: Transform::identity(),
            current_interaction_transforms: HashMap::new(),
            previous_interaction_transforms: HashMap::new(),
            next_overlay_layer: 0,
            frame_overlays: Vec::new(),
            previous_overlays: HashMap::new(),
            current_overlays: HashMap::new(),
            current_layers: HashMap::new(),
            overlay_runtimes: HashMap::new(),
            pending_focus_layer: None,
            requested_animation: false,
            last_frame_stats: FrameStats::default(),
            direction: Direction::Ltr,
            embedded_overlay: false,
            widgets_disabled: false,
            form_label_width: None,
            disabled_theme_backup: None,
            pixel_scale: 1.0,
        }
    }

    /// Sets how many device pixels the host draws per logical pixel, as
    /// MyGo's painter takes its scale.
    pub fn set_pixel_scale(&mut self, scale: f32) {
        self.pixel_scale = if scale.is_finite() { scale.clamp(1.0, 4.0) } else { 1.0 };
    }

    /// Returns the device pixels per logical pixel of this frame.
    pub fn pixel_scale(&self) -> f32 {
        self.pixel_scale
    }

    /// Returns the current logical layout size.
    pub fn available_size(&self) -> Size {
        self.available_size
    }

    /// Returns the latest logical pointer position.
    pub fn pointer_position(&self) -> Option<Point> {
        self.input.pointer
    }

    /// Returns whether the primary pointer is currently held down.
    pub fn pointer_down(&self) -> bool {
        self.input.pointer_down
    }

    /// Returns wheel movement accumulated for this frame.
    pub fn wheel_delta(&self) -> (f32, f32) {
        self.input.wheel
    }

    /// Returns text accumulated for this frame.
    pub fn text_input(&self) -> &str {
        &self.input.text
    }

    /// Overrides the stable key of the next widget.
    ///
    /// Keys must be unique. Automatic ids derive from construction order and
    /// can shift when conditional widgets change order; duplicate keys share
    /// interaction, focus, and persistent widget state.
    pub fn id(&mut self, stable: &str) -> &mut Self {
        self.explicit_id = Some(stable.to_string());
        self
    }

    /// Returns whether the pointer is over the previous-frame region for `id`.
    pub fn hover(&mut self, id_hint: &str) -> bool {
        if !self.pointer_available(self.current_layer) {
            return false;
        }
        self.pointer_in_previous(id_hint)
    }

    /// Applies a transform scope to the widgets constructed inside it.
    ///
    /// The transform is applied per constructed node and recorded for pointer
    /// hit-testing. This is intended for leaf controls such as buttons and
    /// sliders; containers with several children inherit the transform around
    /// each child origin rather than as one group.
    pub fn transform(&mut self, transform: Transform, content: impl FnOnce(&mut Self)) {
        let previous = self.current_transform;
        self.current_transform = compose_transform(previous, transform);
        content(self);
        self.current_transform = previous;
    }

    /// Returns whether `id` is currently focused.
    pub fn focus(&mut self, id: &str) -> bool {
        self.focused_id.as_deref() == Some(id)
    }

    /// Returns whether the most recently constructed focusable widget is focused.
    pub fn is_focused(&self) -> bool {
        self.current_focus_key
            .as_deref()
            .map(|key| self.focused_id.as_deref() == Some(key))
            .unwrap_or(false)
    }

    /// Requests focus for a stable widget id.
    pub fn request_focus(&mut self, id: &str) {
        self.focused_id = Some(id.to_string());
    }

    /// Returns the active theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Disables pointer/keyboard response and lowers widget contrast until
    /// the matching `false` call.
    pub fn set_widgets_disabled(&mut self, disabled: bool) {
        if disabled {
            if self.disabled_theme_backup.is_none() {
                self.disabled_theme_backup = Some(self.theme.clone());
            }
            self.theme.foreground = self.theme.muted;
            self.theme.accent = mix_color(self.theme.surface, self.theme.border, 0.55);
            self.theme.on_accent = self.theme.muted;
            self.widgets_disabled = true;
        } else if self.widgets_disabled {
            if let Some(backup) = self.disabled_theme_backup.take() {
                self.theme = backup;
            }
            self.widgets_disabled = false;
        }
    }

    pub(crate) fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Returns the timestamp for the current frame.
    pub fn frame_time(&self) -> Instant {
        self.frame_time
    }

    /// Returns the active layout direction.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Sets the layout direction for widgets constructed after this call.
    pub fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    /// Runs `content` under a temporary direction and restores the previous one.
    pub fn with_direction(&mut self, direction: Direction, content: impl FnOnce(&mut Context)) {
        let previous = self.direction;
        self.direction = direction;
        content(self);
        self.direction = previous;
    }

    /// Returns timing statistics from the most recently produced frame.
    pub fn frame_stats(&self) -> FrameStats {
        self.last_frame_stats
    }

    /// Migrates context-owned widget state from one stable id to another.
    ///
    /// Covers text-field/select/scroll/slider transient state, animation keys
    /// prefixed by the old id, overlay runtimes, and focus. If `new_id` already
    /// has widget state, or ids are equal, nothing is moved and `false` is
    /// returned.
    pub fn migrate_state(&mut self, old_id: &str, new_id: &str) -> bool {
        if old_id == new_id || self.widget_states.contains_key(new_id) {
            return false;
        }
        let mut moved = false;
        if let Some(state) = self.widget_states.remove(old_id) {
            self.widget_states.insert(new_id.to_string(), state);
            moved = true;
        }
        let mut animation_keys = self
            .animations
            .keys()
            .filter(|key| *key == old_id || key.starts_with(&format!("{old_id}:")))
            .cloned()
            .collect::<Vec<_>>();
        animation_keys.sort();
        for key in animation_keys {
            if self.animations.contains_key(new_id)
                || self
                    .animations
                    .contains_key(&key.replacen(old_id, new_id, 1))
            {
                continue;
            }
            if let Some(value) = self.animations.remove(&key) {
                let new_key = key.replacen(old_id, new_id, 1);
                self.animations.insert(new_key, value);
                moved = true;
            }
        }
        if self.focused_id.as_deref() == Some(old_id) {
            self.focused_id = Some(new_id.to_string());
            moved = true;
        }
        if !self.overlay_runtimes.contains_key(new_id) {
            if let Some(runtime) = self.overlay_runtimes.remove(old_id) {
                self.overlay_runtimes.insert(new_id.to_string(), runtime);
                moved = true;
            }
        }
        moved
    }

    /// Draws a compact previous-frame timing line.
    pub fn frame_stats_panel(&mut self, id: &str, stats: FrameStats) -> FrameStatsResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let text = stats.summary();
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(measured_text_height(theme.font_sizes.small) + theme.spacing.xs * 2.0),
                ..Style::default()
            },
        );
        self.update_text(
            node,
            &text,
            theme.font_sizes.small,
            400,
            theme.muted,
            Some(width),
            TextAlignment::Start,
            0.0,
            false,
            false,
            None,
        );
        self.register_access_node(node, Role::Label, "frame stats", Some(text.as_str()), false);
        FrameStatsResponse { text }
    }

    /// Requests one more frame even without a pending animation.
    ///
    /// Spinners, indeterminate progress, and other frame-clock-driven widgets
    /// use this while they are visible. The flag is per frame and is reported
    /// through [`super::native::ViewFrame::needs_animation_frame`].
    pub fn request_animation_frame(&mut self) {
        self.requested_animation = true;
    }

    /// Returns whether reduced motion is requested.
    ///
    /// This checks `MYRUST_REDUCE_MOTION` and the neutral appearance layer.
    /// Native macOS and Windows reduced-motion APIs are not implemented, so
    /// those platforms currently rely on the environment variable.
    pub fn reduce_motion(&self) -> bool {
        self.reduce_motion
    }

    /// Advances a named scalar toward `target` using a 120ms ease-out animation.
    ///
    /// Motion is skipped when [`Self::reduce_motion`] is true. Animation
    /// timing uses [`Self::frame_time`], not a private timer.
    pub fn animate(&mut self, id: &str, target: f32) -> f32 {
        self.animate_with(
            id,
            target,
            Animation::new(Duration::from_millis(120), Easing::EaseOutCubic),
        )
    }

    /// Advances a named scalar with explicit duration and easing.
    pub fn animate_with(&mut self, id: &str, target: f32, animation: Animation) -> f32 {
        let target = if target.is_finite() { target } else { 0.0 };
        if self.reduce_motion {
            let state = self
                .animations
                .entry(id.to_string())
                .or_insert(AnimatedValue {
                    from: target,
                    current: target,
                    target,
                    started_at: self.frame_time,
                    active: false,
                    last_seen_frame: self.frame_serial,
                });
            state.from = target;
            state.current = target;
            state.target = target;
            state.active = false;
            state.last_seen_frame = self.frame_serial;
            return target;
        }
        let now = self.frame_time;
        if !self.animations.contains_key(id) {
            self.animations.insert(
                id.to_string(),
                AnimatedValue {
                    from: target,
                    current: target,
                    target,
                    started_at: now,
                    active: false,
                    last_seen_frame: self.frame_serial,
                },
            );
            return target;
        }
        let state = self.animations.get_mut(id).expect("animation exists");
        state.last_seen_frame = self.frame_serial;
        if (state.target - target).abs() > f32::EPSILON {
            state.from = state.current;
            state.target = target;
            state.started_at = now;
            state.active = true;
        }
        if state.active {
            let elapsed = now.duration_since(state.started_at).as_secs_f32();
            let duration = animation.duration.as_secs_f32().max(f32::EPSILON);
            let fraction = (elapsed / duration).clamp(0.0, 1.0);
            let eased = animation.value(fraction);
            state.current = state.from + (state.target - state.from) * eased;
            if fraction >= 1.0 {
                state.current = state.target;
                state.active = false;
            }
        }
        state.current
    }

    /// Registers accessibility metadata for the most recently constructed node.
    pub fn access_node(&mut self, role: Role, label: &str, value: Option<&str>) {
        let node_id = self.next_id.saturating_sub(1);
        let focused = self
            .focused_id
            .as_deref()
            .map(|id| id == self.node_key(node_id))
            .unwrap_or(false);
        self.register_access_node(node_id, role, label, value, focused);
    }

    /// Starts a vertical flex container.
    pub fn column(&mut self) -> ColumnBuilder<'_> {
        let style = Style {
            flex_direction: FlexDirection::Column,
            ..Style::default()
        };
        let builder = ColumnBuilder::new(self, style);
        builder
            .ctx
            .register_access_node(builder.id, Role::Group, "column", None, false);
        builder
    }

    /// Starts a grid with equal-width columns and uniform row heights.
    pub fn grid(&mut self) -> GridBuilder<'_> {
        let style = Style {
            display_grid: true,
            grid_columns: 2,
            ..Style::default()
        };
        let builder = FlexBuilder::new(self, style);
        builder
            .ctx
            .register_access_node(builder.id, Role::Group, "grid", None, false);
        builder
    }

    /// Starts a horizontal flex container.
    ///
    /// Its children center across it, as MyGo's `Row()` sets `align` to
    /// `Center` when it is left automatic (`ui/element.go` Row), rather than
    /// stretching: a row of buttons keeps each button at its own height.
    pub fn row(&mut self) -> RowBuilder<'_> {
        let style = Style {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            ..Style::default()
        };
        let builder = RowBuilder::new(self, style);
        builder
            .ctx
            .register_access_node(builder.id, Role::Group, "row", None, false);
        builder
    }

    /// Starts a neutral container.
    pub fn container(&mut self) -> ContainerBuilder<'_> {
        let builder = ContainerBuilder::new(self, Style::default());
        builder
            .ctx
            .register_access_node(builder.id, Role::Group, "container", None, false);
        builder
    }

    /// Starts a clickable container: one that reports whether the pointer is
    /// over it and whether it was clicked this frame, so a demo can lay out
    /// its own rows with MyGo's geometry and still take input.
    pub fn clickable(&mut self, id: &str) -> ClickableBuilder<'_> {
        let node = self.new_named_node(id, Style::default());
        self.register_interaction(node, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let clicked = self.clicked_in_previous(id);
        ClickableBuilder {
            builder: FlexBuilder { ctx: self, id: node },
            hovered,
            clicked,
        }
    }

    /// Creates a text widget.
    pub fn text(&mut self, content: &str) -> TextBuilder<'_> {
        let id = self.new_node(Style::default());
        self.register_access_node(id, Role::Label, content, None, false);
        let font_size = self.theme.font_sizes.body;
        let color = self.theme.foreground;
        let alignment = if self.direction.is_rtl() {
            TextAlignment::End
        } else {
            TextAlignment::Start
        };
        let mut builder = TextBuilder {
            ctx: self,
            id,
            text: content.to_string(),
            font_size,
            weight: 400,
            color,
            max_width: None,
            alignment,
            underline: false,
            line_height: None,
            single_line: false,
            mono: false,
            family: None,
        };
        builder.sync();
        builder
    }

    /// Creates a button and returns its click state for this frame.
    pub fn button(&mut self, label: &str) -> ButtonState {
        self.button_styled(label, false)
    }

    /// Creates a filled primary button using the accent color.
    pub fn button_primary(&mut self, label: &str) -> ButtonState {
        self.button_styled(label, true)
    }

    /// Creates a button with a built-in icon before the label.
    pub fn button_with_icon(&mut self, label: &str, icon_name: &str) -> ButtonState {
        self.button_icon_impl(label, icon_name, false, false, None, 0.0)
    }

    /// Creates a primary button with a built-in icon.
    pub fn button_primary_with_icon(&mut self, label: &str, icon_name: &str) -> ButtonState {
        self.button_icon_impl(label, icon_name, true, false, None, 0.0)
    }

    /// Creates a button with a built-in icon before the label.
    /// Creates a secondary button with a built-in icon after the label,
    /// drawn at MyGo's 10px menu chevron size in the muted color.
    pub fn button_with_icon_after(&mut self, label: &str, icon_name: &str) -> ButtonState {
        let muted = self.theme.muted;
        self.button_icon_impl(label, icon_name, false, true, Some(muted), 10.0)
    }

    fn button_icon_impl(
        &mut self,
        label: &str,
        icon_name: &str,
        primary: bool,
        icon_after: bool,
        icon_color: Option<Color>,
        icon_size: f32,
    ) -> ButtonState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let text_size = estimated_text_size(label, font_size, None);
        // MyGo's styleButton: Space(1.5)=6 vertical and Space(3.5)=14
        // horizontal, the border taking a pixel more on a plain button so
        // that it stands as tall as the emphasized primary one.
        let (vertical, horizontal) = if primary { (6.0, 14.0) } else { (7.0, 15.0) };
        // An icon inside a button takes the button's text size.
        let icon_size = if icon_size > 0.0 { icon_size } else { font_size };
        let icon_gap = theme.spacing.base * 1.5;
        // MyGo's button base is a centered row, so the label's own line box
        // stands beside the icon rather than around it.
        let content_size = Size::new(
            text_size.width + icon_size + icon_gap,
            text_size.height.max(icon_size),
        );
        let id = self.new_node(Style {
            padding: Edges::symmetric(vertical, horizontal),
            ..Style::default()
        });
        let key = self
            .nodes
            .get(&id)
            .and_then(|node| node.key.clone())
            .unwrap_or_else(|| format!("button-{id}"));
        let previous = self.previous_regions.get(&key).cloned();
        let mouse_clicked = self.clicked_in_previous(&key);
        let hovered = self.pointer_in_previous(&key);
        let pressed = hovered && self.input.pointer_down;
        let focused = self.focused_id.as_deref() == Some(key.as_str());
        let clicked = mouse_clicked
            || (focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space)));
        if pressed || clicked {
            self.focused_id = Some(key.clone());
        }
        let focused = self.focused_id.as_deref() == Some(key.as_str());
        let hover_progress = self.animate(&format!("{key}:hover"), if hovered { 1.0 } else { 0.0 });
        let pressed_progress =
            self.animate(&format!("{key}:pressed"), if pressed { 1.0 } else { 0.0 });
        let bounds = previous.as_ref().map(|region| region.bounds);
        let (base_bg, hover_bg, pressed_bg) = if primary {
            (theme.accent, theme.accent_hover, theme.accent_pressed)
        } else {
            (theme.surface, theme.surface_hover, theme.surface_pressed)
        };
        let background = mix_color(
            mix_color(base_bg, hover_bg, hover_progress.clamp(0.0, 1.0)),
            pressed_bg,
            pressed_progress.clamp(0.0, 1.0),
        );
        let emphasis = hover_progress.max(pressed_progress);
        let _ = emphasis;
        let text_color = if primary {
            theme.on_accent
        } else {
            theme.foreground
        };
        let total_w = content_size.width + horizontal * 2.0;
        let total_h = content_size.height + vertical * 2.0;
        // As MyGo's styleButton: a plain button is a Surface face with a
        // 1px Border, a primary one a borderless accent face.
        let mut commands = Vec::new();
        if !primary {
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: total_w,
                height: total_h,
                radius: Radius::all(6.0),
                color: theme.border,
            });
        }
        commands.push(DrawCommand::RoundedRect {
            x: if primary { 0.0 } else { 1.0 },
            y: if primary { 0.0 } else { 1.0 },
            width: if primary { total_w } else { (total_w - 2.0).max(1.0) },
            height: if primary { total_h } else { (total_h - 2.0).max(1.0) },
            radius: Radius::all(if primary { 6.0 } else { 5.0 }),
            color: background,
        });
        // Icon, before or after the label
        if let Some(icon) = Icon::builtin(icon_name) {
            let icon_x = if icon_after {
                horizontal + text_size.width + icon_gap
            } else {
                horizontal
            };
            let icon_y = vertical + (content_size.height - icon_size) / 2.0;
            commands.push(DrawCommand::TransformPush {
                transform: Transform {
                    translation: Point::new(icon_x, icon_y),
                    scale: Point::new(1.0, 1.0),
                    rotation: 0.0,
                },
            });
            let color = icon_color.unwrap_or(text_color);
            commands.extend(icon.to_commands_with_direction(icon_size, color, self.direction));
            commands.push(DrawCommand::TransformPop);
        }
        // Label
        commands.push(DrawCommand::Text {
            x: horizontal + icon_size + icon_gap,
            y: vertical,
            text: label.to_string(),
            font_size,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: text_color,
            max_width: None,
            alignment: TextAlignment::Start,
        });
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, total_w, total_h, theme.accent));
        }
        let node = self.nodes.get_mut(&id).expect("button node exists");
        node.content_size = content_size;
        node.commands = commands;
        self.register_interaction(id, key.clone());
        self.register_focusable(id, key.clone());
        self.register_access_node(id, Role::Button, label, None, focused);
        ButtonState {
            id: key,
            clicked,
            pressed,
            focused,
            hovered,
            bounds,
        }
    }

    /// Creates a 36×36 icon-only button.
    pub fn icon_button(&mut self, icon_name: &str) -> ButtonState {
        let theme = self.theme.clone();
        // MyGo's icon buttons are ordinary buttons whose content is the
        // icon: `Padding(Space(1.5), Space(3.5))` around a `Border(1)`, and
        // an icon one em high (`intrinsicSize`: `em := e.resolvedText().size`),
        // so 14 + 2 * (6 + 1) tall and 14 + 2 * (14 + 1) wide.
        let icon_size = 14.0;
        let width = 44.0;
        let height = 28.0;
        let id = self.new_node(Style {
            width: Some(width),
            height: Some(height),
            ..Style::default()
        });
        let key = self
            .nodes
            .get(&id)
            .and_then(|node| node.key.clone())
            .unwrap_or_else(|| format!("icon-button-{id}"));
        let previous = self.previous_regions.get(&key).cloned();
        let mouse_clicked = self.clicked_in_previous(&key);
        let hovered = self.pointer_in_previous(&key);
        let pressed = hovered && self.input.pointer_down;
        let focused = self.focused_id.as_deref() == Some(key.as_str());
        let clicked = mouse_clicked
            || (focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space)));
        if pressed || clicked {
            self.focused_id = Some(key.clone());
        }
        let focused = self.focused_id.as_deref() == Some(key.as_str());
        let hover_progress = self.animate(&format!("{key}:hover"), if hovered { 1.0 } else { 0.0 });
        let background = mix_color(theme.surface, mix_color(theme.surface, theme.foreground, 0.12), hover_progress);
        let border = if focused {
            theme.accent
        } else if hovered {
            mix_color(theme.border, theme.accent, 0.35)
        } else {
            theme.border
        };
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(6.0),
                color: border,
            },
            DrawCommand::RoundedRect {
                x: 1.0,
                y: 1.0,
                width: width - 2.0,
                height: height - 2.0,
                radius: Radius::all(5.0),
                color: background,
            },
        ];
        if let Some(icon) = Icon::builtin(icon_name) {
            // The icon is the button's content, centered in its padding box.
            let icon_x = (width - icon_size) / 2.0;
            let icon_y = (height - icon_size) / 2.0;
            commands.push(DrawCommand::TransformPush {
                transform: Transform {
                    translation: Point::new(icon_x, icon_y),
                    scale: Point::new(1.0, 1.0),
                    rotation: 0.0,
                },
            });
            commands.extend(icon.to_commands_with_direction(icon_size, theme.foreground, self.direction));
            commands.push(DrawCommand::TransformPop);
        }
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.nodes.get_mut(&id).expect("icon button exists").commands = commands;
        self.register_interaction(id, key.clone());
        self.register_focusable(id, key.clone());
        self.register_access_node(id, Role::Button, icon_name, None, focused);
        ButtonState {
            id: key,
            clicked,
            pressed,
            focused,
            hovered,
            bounds: previous.map(|region| region.bounds),
        }
    }

    fn button_styled(&mut self, label: &str, primary: bool) -> ButtonState {
        self.button_styled_disabled(label, primary, false)
    }

    /// Creates a button that shows as unavailable when `disabled` is set.
    ///
    /// MyGo dims a disabled element, and what is inside it, to half its
    /// opacity (`ui/paint.go`: `p.opacity *= 0.5`), so its face, border and
    /// label all blend halfway into the page behind them.
    pub fn button_disabled(&mut self, label: &str, disabled: bool) -> ButtonState {
        self.button_styled_disabled(label, false, disabled)
    }

    fn button_styled_disabled(
        &mut self,
        label: &str,
        primary: bool,
        disabled: bool,
    ) -> ButtonState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        // MyGo's buttons pad 6 by 14 over a one-pixel border, 31 tall; the
        // emphasized primary button pads 6 by 14 without a border, 29 tall
        // (`ui/widgets.go` styleButton).
        let (vertical, horizontal) = if primary { (6.0, 14.0) } else { (7.0, 15.0) };
        let id = self.new_node(Style {
            padding: super::layout::Edges::symmetric(vertical, horizontal),
            ..Style::default()
        });
        let content_size = estimated_text_size(label, font_size, None);
        let key = self
            .nodes
            .get(&id)
            .and_then(|node| node.key.clone())
            .unwrap_or_else(|| format!("button-{id}"));
        let previous = self.previous_regions.get(&key).cloned();
        let mouse_clicked = self.clicked_in_previous(&key);
        let hovered = self.pointer_in_previous(&key);
        let pressed = !disabled && hovered && self.input.pointer_down;
        let focused = self.focused_id.as_deref() == Some(key.as_str());
        let clicked = !disabled
            && (mouse_clicked
                || (focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space))));
        if pressed || clicked {
            self.focused_id = Some(key.clone());
        }
        let focused = self.focused_id.as_deref() == Some(key.as_str());
        let hover_progress = self.animate(&format!("{key}:hover"), if hovered { 1.0 } else { 0.0 });
        let pressed_progress =
            self.animate(&format!("{key}:pressed"), if pressed { 1.0 } else { 0.0 });
        let bounds = previous.as_ref().map(|region| region.bounds);
        let node = self.nodes.get_mut(&id).expect("button node exists");
        node.content_size = content_size;
        node.commands = button_commands(
            label,
            font_size,
            &theme,
            hover_progress,
            pressed_progress,
            focused,
            primary,
            disabled,
        );
        self.register_interaction(id, key.clone());
        self.register_focusable(id, key.clone());
        self.register_access_node(id, Role::Button, label, None, focused);
        ButtonState {
            id: key,
            clicked,
            pressed,
            focused,
            hovered,
            bounds,
        }
    }

    /// Creates a compact button whose label sits in `horizontal` by
    /// `vertical` padding, as MyGo's `Button(...).Padding(v, h)`.
    ///
    /// `active` fills the face with the accent color at low alpha, the
    /// mark of a selected formatting button in a toolbar.
    pub fn button_padded(
        &mut self,
        id: &str,
        label: &str,
        horizontal: f32,
        vertical: f32,
        active: bool,
    ) -> ButtonState {
        self.button_padded_disabled(id, label, horizontal, vertical, active, false)
    }

    /// Creates a padded button that shows as unavailable when `disabled` is
    /// set, as MyGo's `Button(...).Padding(v, h).Disabled(b)` does.
    pub fn button_padded_disabled(
        &mut self,
        id: &str,
        label: &str,
        horizontal: f32,
        vertical: f32,
        active: bool,
        disabled: bool,
    ) -> ButtonState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let text_size = estimated_text_size(label, font_size, None);
        let width = text_size.width + horizontal * 2.0 + 2.0;
        let height = text_size.height + vertical * 2.0 + 2.0;
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node, id.to_string());
        self.register_focusable(node, id.to_string());
        let hovered = !disabled && self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        let clicked = !disabled && self.clicked_in_previous(id);
        let focused = self.focused_id.as_deref() == Some(id);
        let hover_progress = self.animate(&format!("{id}:hover"), if hovered { 1.0 } else { 0.0 });
        let pressed_progress =
            self.animate(&format!("{id}:pressed"), if pressed { 1.0 } else { 0.0 });
        let background = if active {
            Color::rgba(theme.accent.r, theme.accent.g, theme.accent.b, 46)
        } else {
            mix_color(
                mix_color(
                    theme.surface,
                    theme.surface_hover,
                    hover_progress.clamp(0.0, 1.0),
                ),
                theme.surface_pressed,
                pressed_progress.clamp(0.0, 1.0),
            )
        };
        // MyGo draws a bordered box as a rounded rect filled with the face,
        // and the one-pixel border as a ring outside it (`internal/scene`
        // OpFill border): a translucent face blends with the page behind it,
        // not with the border's color.
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 1.0,
                y: 1.0,
                width: (width - 2.0).max(1.0),
                height: (height - 2.0).max(1.0),
                radius: Radius::all(5.0),
                color: background,
            },
            rounded_ring(width, height, 6.0, 1.0, theme.border),
            DrawCommand::Text {
                x: 1.0 + horizontal,
                y: (height - measured_text_height(font_size)) / 2.0,
                text: label.to_string(),
                font_size,
                weight: 400,
                italic: false,
                mono: false,
                family: None,
                color: theme.foreground,
                max_width: Some((text_size.width).max(1.0)),
                alignment: TextAlignment::Start,
            },
        ];
        if disabled {
            // Half the opacity over the page, as MyGo dims a disabled
            // element and its contents.
            let dim = |color: Color| mix_color(color, theme.background, 0.5);
            for command in &mut commands {
                match command {
                    DrawCommand::RoundedRect { color, .. }
                    | DrawCommand::Rect { color, .. }
                    | DrawCommand::Line { color, .. } => *color = dim(*color),
                    DrawCommand::Text { color, .. } => *color = dim(*color),
                    _ => {}
                }
            }
        }
        self.nodes
            .get_mut(&node)
            .expect("padded button exists")
            .commands = commands;
        ButtonState {
            id: id.to_string(),
            clicked,
            pressed,
            focused,
            hovered,
            bounds: None,
        }
    }

    /// Creates a focusable interactive region for the most recently built node.
    pub fn focusable(&mut self) -> &mut Self {
        let id = self.next_id.saturating_sub(1);
        let key = self.node_key(id);
        self.register_interaction(id, key.clone());
        self.register_focusable(id, key);
        self
    }

    /// Creates a single-line text field.
    ///
    /// Supports character input, Backspace, Delete, Home/End, and left/right
    /// cursor movement. Selection, clipboard, undo, IME composition, and dead
    /// keys are not implemented.
    /// Returns the cursor, selection, and geometry of a text field.
    pub fn text_input_state(&self, id: &str) -> Option<TextInputState> {
        let (cursor, anchor, cursor_x, cursor_y) = match self.widget_states.get(id) {
            Some(WidgetState::TextField {
                cursor,
                anchor,
                cursor_x,
                cursor_y,
            }) => (*cursor, *anchor, *cursor_x, *cursor_y),
            _ => return None,
        };
        let bounds = self
            .current_regions
            .get(id)
            .or_else(|| self.previous_regions.get(id))?
            .bounds;
        let selection = anchor
            .filter(|anchor| *anchor != cursor)
            .map(|anchor| (anchor.min(cursor), anchor.max(cursor)));
        Some(TextInputState {
            cursor,
            selection,
            bounds,
            cursor_x: bounds.x + cursor_x,
            cursor_y: bounds.y + cursor_y,
        })
    }

    /// Sets a byte-range selection on a text field for custom controls.
    pub fn set_text_input_selection(&mut self, id: &str, start: usize, end: usize) {
        let state = self
            .widget_states
            .entry(id.to_string())
            .or_insert(WidgetState::TextField {
                cursor: end,
                anchor: Some(start),
                cursor_x: 0.0,
                cursor_y: 0.0,
            });
        if let WidgetState::TextField { cursor, anchor, .. } = state {
            *cursor = end;
            *anchor = Some(start);
        }
    }

    /// Creates a text field backed by a caller-owned string.
    pub fn text_field(&mut self, id: &str, value: &mut String) -> TextFieldState {
        self.text_field_impl(id, value, None, false, None, false)
    }

    /// Creates a single-line field whose border shows the Danger color, as
    /// MyGo's text input does inside a field that reports an error
    /// (`ui/form.go` inputBorder).
    pub fn text_field_invalid(
        &mut self,
        id: &str,
        value: &mut String,
        invalid: bool,
    ) -> TextFieldState {
        self.text_field_invalid_impl(id, value, None, false, None, false, invalid)
    }

    /// Creates a single-line field that renders a bullet for each typed
    /// character while keeping the caller's real string intact.
    pub fn text_field_password(&mut self, id: &str, value: &mut String) -> TextFieldState {
        self.text_field_impl(id, value, None, true, None, false)
    }

    /// Creates a single-line field that shows `placeholder` while empty,
    /// in the muted color, as MyGo's `Placeholder`.
    pub fn text_field_placeholder(
        &mut self,
        id: &str,
        value: &mut String,
        placeholder: &str,
    ) -> TextFieldState {
        self.text_field_impl(id, value, None, false, Some(placeholder), false)
    }
    /// Creates a masked field that shows `placeholder` while empty.
    pub fn text_field_password_placeholder(
        &mut self,
        id: &str,
        value: &mut String,
        placeholder: &str,
    ) -> TextFieldState {
        self.text_field_impl(id, value, None, true, Some(placeholder), false)
    }



    /// Creates a text field without its frame, the input inside a MyGo
    /// field (`ui/combobox.go` field): one line, no padding, no border.
    pub fn text_field_bare(&mut self, id: &str, value: &mut String) -> TextFieldState {
        self.text_field_impl(id, value, None, false, None, true)
    }

    /// Creates a borderless single-line field that shows `placeholder`
    /// while empty.
    pub fn text_field_bare_placeholder(
        &mut self,
        id: &str,
        value: &mut String,
        placeholder: Option<&str>,
    ) -> TextFieldState {
        self.text_field_impl(id, value, None, false, placeholder, true)
    }

    fn text_field_impl(
        &mut self,
        id: &str,
        value: &mut String,
        filter: Option<&dyn Fn(&str) -> String>,
        mask: bool,
        placeholder: Option<&str>,
        bare: bool,
    ) -> TextFieldState {
        self.text_field_invalid_impl(id, value, filter, mask, placeholder, bare, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn text_field_invalid_impl(
        &mut self,
        id: &str,
        value: &mut String,
        filter: Option<&dyn Fn(&str) -> String>,
        mask: bool,
        placeholder: Option<&str>,
        bare: bool,
        invalid: bool,
    ) -> TextFieldState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let measured = measured_text_height(font_size);
        // MyGo's text input: Space(1.5) of vertical and Space(2.5) of
        // horizontal padding around one line, on a Surface box edged in
        // Border (`ui/textinput.go` textInput).
        // A bare field draws its line alone, as the input inside a MyGo
        // field (`ui/combobox.go` field).
        let vertical = if bare { 0.0 } else { 6.0 };
        let horizontal = if bare { 0.0 } else { 10.0 };
        let inset = if bare { 0.0 } else { 1.0 };
        // MyGo's Border(1) takes layout space, so the box is two pixels
        // taller than the padding and one line alone.
        let height = measured + vertical * 2.0 + if bare { 0.0 } else { 2.0 };
        self.widget_state_text_field(id);
        let width = self.current_content_width();
        let style = Style {
            width: Some(width),
            height: Some(height),
            padding: super::layout::Edges::symmetric(vertical, horizontal),
            ..Style::default()
        };
        let node_id = self.new_named_node(id, style);
        self.register_interaction(node_id, id.to_string());
        self.register_focusable(node_id, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let mut changed = false;
        let (mut cursor, mut anchor) = match self.widget_states.get(id) {
            Some(WidgetState::TextField { cursor, anchor, .. }) => (*cursor, *anchor),
            _ => (0, None),
        };
        cursor = cursor.min(value.len());
        while !value.is_char_boundary(cursor) {
            cursor -= 1;
        }
        if let Some(mut anchor_value) = anchor {
            anchor_value = anchor_value.min(value.len());
            while !value.is_char_boundary(anchor_value) {
                anchor_value = anchor_value.saturating_sub(1);
            }
            anchor = Some(anchor_value);
        }
        let mut selection = anchor
            .filter(|anchor| *anchor != cursor)
            .map(|anchor| (anchor.min(cursor), anchor.max(cursor)));
        if focused {
            let text = self.input.text.clone();
            if !text.is_empty() {
                if let Some((start, end)) = selection {
                    value.replace_range(start..end, &text);
                    cursor = start + text.len();
                } else {
                    value.insert_str(cursor, &text);
                    cursor += text.len();
                }
                anchor = None;
                selection = None;
                changed = true;
            }
            for key in self.input.keys.clone() {
                match key {
                    Key::Backspace => {
                        if let Some((start, end)) = selection {
                            value.replace_range(start..end, "");
                            cursor = start;
                            changed = true;
                        } else if let Some(previous) = previous_char_boundary(value, cursor) {
                            value.replace_range(previous..cursor, "");
                            cursor = previous;
                            changed = true;
                        }
                        anchor = None;
                        selection = None;
                    }
                    Key::Delete => {
                        if let Some((start, end)) = selection {
                            value.replace_range(start..end, "");
                            cursor = start;
                            changed = true;
                        } else if let Some(next) = next_char_boundary(value, cursor) {
                            value.replace_range(cursor..next, "");
                            changed = true;
                        }
                        anchor = None;
                        selection = None;
                    }
                    Key::ArrowLeft => {
                        if let Some(previous) = previous_char_boundary(value, cursor) {
                            cursor = previous;
                        }
                        anchor = None;
                        selection = None;
                    }
                    Key::ArrowRight => {
                        if let Some(next) = next_char_boundary(value, cursor) {
                            cursor = next;
                        }
                        anchor = None;
                        selection = None;
                    }
                    Key::Home => {
                        cursor = 0;
                        anchor = None;
                        selection = None;
                    }
                    Key::End => {
                        cursor = value.len();
                        anchor = None;
                        selection = None;
                    }
                    _ => {}
                }
            }
        }
        if let Some(filter) = filter {
            let filtered = filter(value.as_str());
            if filtered != *value {
                *value = filtered;
                cursor = value.len();
                anchor = None;
                changed = true;
            }
            cursor = cursor.min(value.len());
            while !value.is_char_boundary(cursor) {
                cursor -= 1;
            }
        }
        let display: String = if mask {
            value.chars().map(|_| '•').collect()
        } else {
            value.clone()
        };
        let display_cursor_prefix: String = if mask {
            value[..cursor].chars().map(|_| '•').collect()
        } else {
            value[..cursor].to_string()
        };
        let cursor_x = horizontal + inset + measure_text_width(&display_cursor_prefix, font_size);
        let cursor_y = vertical + inset;
        self.widget_states.insert(
            id.to_string(),
            WidgetState::TextField {
                cursor,
                anchor,
                cursor_x,
                cursor_y,
            },
        );
        // The box is the layout's own: the border takes layout space, so
        // the face must fill the node rather than the padding alone.
        let height = measured + vertical * 2.0 + if bare { 0.0 } else { 2.0 };
        // MyGo's inputBorder (`ui/form.go`): the Danger color while the
        // input is invalid, the accent while it has the focus, a darker
        // border while the pointer is over it.
        let border = if invalid {
            theme.danger()
        } else if focused {
            theme.accent
        } else if hovered {
            mix_color(theme.border, theme.accent, 0.35)
        } else {
            theme.border
        };
        let mut commands = if bare {
            Vec::new()
        } else {
            vec![
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                    radius: Radius::all(6.0),
                    color: border,
                },
                DrawCommand::RoundedRect {
                    x: 1.0,
                    y: 1.0,
                    width: (width - 2.0).max(1.0),
                    height: (height - 2.0).max(1.0),
                    radius: Radius::all(5.0),
                    color: theme.surface,
                },
            ]
        };
        if focused {
            if let Some((start, end)) = selection {
                let selection_prefix: String = if mask {
                    value[..start].chars().map(|_| '•').collect()
                } else {
                    value[..start].to_string()
                };
                let selection_text: String = if mask {
                    value[start..end].chars().map(|_| '•').collect()
                } else {
                    value[start..end].to_string()
                };
                let selection_x = horizontal + measure_text_width(&selection_prefix, font_size);
                let selection_width = measure_text_width(&selection_text, font_size);
                commands.push(DrawCommand::Rect {
                    x: selection_x,
                    y: vertical - 1.0,
                    width: selection_width.max(1.0),
                    height: measured_text_height(font_size) + 2.0,
                    // MyGo's Selection: the accent at a quarter alpha.
                    color: Color::rgba(theme.accent.r, theme.accent.g, theme.accent.b, 64),
                });
            }
        }
        // An empty field shows its placeholder in the muted color, as MyGo's
        // editor does.
        let empty = display.is_empty();
        let (shown, shown_color) = if empty {
            match placeholder {
                Some(placeholder) if !placeholder.is_empty() => {
                    (placeholder.to_string(), theme.muted)
                }
                _ => (display.clone(), theme.foreground),
            }
        } else {
            (display.clone(), theme.foreground)
        };
        commands.push(DrawCommand::Text {
            x: horizontal + inset,
            y: vertical + inset,
            text: shown,
            font_size,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: shown_color,
            max_width: Some((width - horizontal * 2.0).max(1.0)),
            alignment: TextAlignment::Start,
        });
        if self.shows_focus_ring(focused) && !bare {
            commands.extend(focus_ring(0.0, 0.0, width, height, border));
        }
        if focused {
            let cursor_x = horizontal + inset + measure_text_width(&display_cursor_prefix, font_size);
            commands.push(DrawCommand::Line {
                x1: cursor_x,
                y1: vertical + inset,
                x2: cursor_x,
                y2: height - vertical - inset,
                width: 1.0,
                color: theme.accent,
            });
        }
        self.register_access_node(
            node_id,
            Role::TextField,
            "text field",
            Some(display.as_str()),
            focused,
        );
        let text_ascent = host_text_ascent(font_size, false, false, false)
            .or_else(|| default_font().map(|font| font.ascent(font_size)))
            .unwrap_or(font_size * 0.93);
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.commands = commands;
            // MyGo lines a form field's row up on the first line of text it
            // finds inside the control (`ui/form.go` anchor), which for a
            // text input is its line inside the padding and border.
            node.first_baseline = Some(vertical + inset + text_ascent);
        }
        TextFieldState {
            changed,
            cursor,
            focused,
            hovered,
        }
    }

    /// Creates a multiline text area backed by a caller-owned string.
    ///
    /// Enter inserts a newline, Backspace/Delete cross line boundaries,
    /// ArrowUp/ArrowDown preserve the column where possible, Home/End operate
    /// on the current line, and the visible window follows the cursor.
    pub fn text_area(
        &mut self,
        id: &str,
        value: &mut String,
        visible_lines: usize,
    ) -> TextFieldState {
        self.text_area_impl(id, value, visible_lines, None, 80.0, false)
    }

    /// Creates a multiline text area that shows `placeholder` while empty.
    pub fn text_area_placeholder(
        &mut self,
        id: &str,
        value: &mut String,
        visible_lines: usize,
        placeholder: &str,
    ) -> TextFieldState {
        self.text_area_impl(id, value, visible_lines, Some(placeholder), 80.0, false)
    }
    /// Creates a multiline text area at least `min_height` tall, as MyGo's
    /// `Height` on a text area.
    pub fn text_area_sized(
        &mut self,
        id: &str,
        value: &mut String,
        visible_lines: usize,
        min_height: f32,
    ) -> TextFieldState {
        self.text_area_impl(id, value, visible_lines, None, min_height, false)
    }

    /// Creates a multiline text area at least `min_height` tall that shows
    /// `placeholder` while empty, as MyGo's `TextArea(...).Placeholder`.
    pub fn text_area_sized_placeholder(
        &mut self,
        id: &str,
        value: &mut String,
        visible_lines: usize,
        min_height: f32,
        placeholder: &str,
    ) -> TextFieldState {
        self.text_area_impl(
            id,
            value,
            visible_lines,
            Some(placeholder),
            min_height,
            false,
        )
    }

    /// Creates a multiline text area in a monospace font, as MyGo's
    /// `TextAreaBuffer(...).Font("monospace")`.
    pub fn text_area_mono(
        &mut self,
        id: &str,
        value: &mut String,
        visible_lines: usize,
        min_height: f32,
    ) -> TextFieldState {
        self.text_area_impl(id, value, visible_lines, None, min_height, true)
    }



    fn text_area_impl(
        &mut self,
        id: &str,
        value: &mut String,
        visible_lines: usize,
        placeholder: Option<&str>,
        min_height: f32,
        mono: bool,
    ) -> TextFieldState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        // MyGo lays each line as tall as its font's ascent and descent; the
        // monospace family is a pixel shorter than the sans one.
        let line_height = measured_text_height_styled(font_size, mono);
        let visible = visible_lines.max(1);
        let vertical = 6.0;
        let horizontal = 10.0;
        // MyGo's text area sets MinHeight Space(20) (`ui/textinput.go`).
        let height = (line_height * visible as f32 + vertical * 2.0).max(min_height);
        let width = self.current_content_width();
        self.widget_state_text_field(id);
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                padding: super::layout::Edges::symmetric(vertical, horizontal),
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.register_focusable(node_id, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let mut changed = false;
        let (mut cursor, mut anchor) = match self.widget_states.get(id) {
            Some(WidgetState::TextField { cursor, anchor, .. }) => (*cursor, *anchor),
            _ => (0, None),
        };
        cursor = cursor.min(value.len());
        while !value.is_char_boundary(cursor) {
            cursor -= 1;
        }
        if let Some(mut anchor_value) = anchor {
            anchor_value = anchor_value.min(value.len());
            while !value.is_char_boundary(anchor_value) {
                anchor_value = anchor_value.saturating_sub(1);
            }
            anchor = Some(anchor_value);
        }
        let mut selection = anchor
            .filter(|anchor| *anchor != cursor)
            .map(|anchor| (anchor.min(cursor), anchor.max(cursor)));
        if focused {
            let text = self.input.text.clone();
            if !text.is_empty() {
                if let Some((start, end)) = selection {
                    value.replace_range(start..end, &text);
                    cursor = start + text.len();
                } else {
                    value.insert_str(cursor, &text);
                    cursor += text.len();
                }
                anchor = None;
                selection = None;
                changed = true;
            }
            for key in self.input.keys.clone() {
                match key {
                    Key::Enter => {
                        if let Some((start, end)) = selection {
                            value.replace_range(start..end, "\n");
                            cursor = start + 1;
                        } else {
                            value.insert(cursor, '\n');
                            cursor += 1;
                        }
                        anchor = None;
                        selection = None;
                        changed = true;
                    }
                    Key::Backspace => {
                        if let Some((start, end)) = selection {
                            value.replace_range(start..end, "");
                            cursor = start;
                            changed = true;
                        } else if let Some(previous) = previous_char_boundary(value, cursor) {
                            value.replace_range(previous..cursor, "");
                            cursor = previous;
                            changed = true;
                        }
                        anchor = None;
                        selection = None;
                    }
                    Key::Delete => {
                        if let Some((start, end)) = selection {
                            value.replace_range(start..end, "");
                            cursor = start;
                            changed = true;
                        } else if let Some(next) = next_char_boundary(value, cursor) {
                            value.replace_range(cursor..next, "");
                            changed = true;
                        }
                        anchor = None;
                        selection = None;
                    }
                    Key::ArrowLeft => {
                        if let Some(previous) = previous_char_boundary(value, cursor) {
                            cursor = previous;
                        }
                        anchor = None;
                        selection = None;
                    }
                    Key::ArrowRight => {
                        if let Some(next) = next_char_boundary(value, cursor) {
                            cursor = next;
                        }
                        anchor = None;
                        selection = None;
                    }
                    Key::ArrowUp | Key::ArrowDown => {
                        let spans = line_spans(value);
                        let current = line_index(&spans, cursor);
                        let (start, _end) = spans[current];
                        let column = value[start..cursor].chars().count();
                        let target = if matches!(key, Key::ArrowUp) {
                            current.checked_sub(1)
                        } else if current + 1 < spans.len() {
                            Some(current + 1)
                        } else {
                            None
                        };
                        if let Some(target) = target {
                            let (target_start, target_end) = spans[target];
                            cursor = byte_at_column(value, target_start, target_end, column);
                        }
                        anchor = None;
                        selection = None;
                    }
                    Key::Home => {
                        let spans = line_spans(value);
                        cursor = spans[line_index(&spans, cursor)].0;
                        anchor = None;
                        selection = None;
                    }
                    Key::End => {
                        let spans = line_spans(value);
                        cursor = spans[line_index(&spans, cursor)].1;
                        anchor = None;
                        selection = None;
                    }
                    _ => {}
                }
            }
        }
        let spans = line_spans(value);
        let current_line = line_index(&spans, cursor);
        let start_line = if spans.len() > visible {
            (current_line + 1).saturating_sub(visible)
        } else {
            0
        };
        let (line_start, _) = spans[current_line];
        let cursor_x = horizontal + 1.0
            + host_text_width_weighted(&value[line_start..cursor], font_size, false, mono, 400)
                .unwrap_or_else(|| measure_text_width(&value[line_start..cursor], font_size));
        let cursor_y = vertical + 1.0 + (current_line - start_line) as f32 * line_height;
        self.widget_states.insert(
            id.to_string(),
            WidgetState::TextField {
                cursor,
                anchor,
                cursor_x,
                cursor_y,
            },
        );
        let border = if focused {
            theme.accent
        } else if hovered {
            mix_color(theme.border, theme.accent, 0.35)
        } else {
            theme.border
        };
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(6.0),
                color: border,
            },
            DrawCommand::RoundedRect {
                x: 1.0,
                y: 1.0,
                width: (width - 2.0).max(1.0),
                height: (height - 2.0).max(1.0),
                radius: Radius::all(5.0),
                color: theme.surface,
            },
        ];
        if value.is_empty() {
            if let Some(placeholder) = placeholder {
                if !placeholder.is_empty() {
                    // MyGo's border takes layout space, so the text starts
                    // one pixel inside the padding (`ui/textinput.go`).
                    commands.push(DrawCommand::Text {
                        x: horizontal + 1.0,
                        y: vertical + 1.0,
                        text: placeholder.to_string(),
                        font_size,
                        weight: 400,
                        italic: false,
                        mono,
                        family: None,
                        color: theme.muted,
                        max_width: Some((width - horizontal * 2.0).max(1.0)),
                        alignment: TextAlignment::Start,
                    });
                }
            }
        }
        let line_count = spans.len();
        for line in start_line..line_count.min(start_line + visible) {
            let (start, end) = spans[line];
            commands.push(DrawCommand::Text {
                x: horizontal + 1.0,
                y: vertical + 1.0 + (line - start_line) as f32 * line_height,
                text: value[start..end].to_string(),
                font_size,
                weight: 400,
                italic: false,
                mono,
                family: None,
                color: theme.foreground,
                max_width: Some((width - horizontal * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
        }
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, border));
        }
        if focused {
            commands.push(DrawCommand::Line {
                x1: cursor_x,
                y1: cursor_y,
                x2: cursor_x,
                y2: cursor_y + line_height,
                width: 1.0,
                color: theme.accent,
            });
        }
        self.register_access_node(
            node_id,
            Role::TextField,
            "text area",
            Some(value.as_str()),
            focused,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("text area node exists")
            .commands = commands;
        // The area's first line, for a row that lines children up on
        // baselines (`ui/form.go` anchor).
        let area_ascent = host_text_ascent(font_size, false, mono, false)
            .or_else(|| default_font().map(|font| font.ascent(font_size)))
            .unwrap_or(font_size * 0.93);
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.first_baseline = Some(vertical + 1.0 + area_ascent);
        }
        TextFieldState {
            changed,
            cursor,
            focused,
            hovered,
        }
    }

    /// Creates a label that switches into in-place text editing.
    pub fn editable_text(
        &mut self,
        id: &str,
        state: &mut EditableTextState,
        text: &mut String,
    ) -> EditableTextResponse {
        let theme = self.theme.clone();
        let input_id = format!("{id}:input");
        let mut submitted = false;
        let mut cancelled = false;
        let mut changed = false;
        if !state.editing {
            let clicked = self.clicked_in_previous(id);
            let start = clicked;
            if start {
                state.editing = true;
                state.buffer.clone_from(text);
                state.was_focused = true;
                self.focused_id = Some(input_id.clone());
                self.widget_states.insert(
                    input_id.clone(),
                    WidgetState::TextField {
                        cursor: state.buffer.len(),
                        anchor: None,
                        cursor_x: 0.0,
                        cursor_y: 0.0,
                    },
                );
            }
        }
        if state.editing {
            if !state.was_focused && self.focused_id.as_deref() != Some(input_id.as_str()) {
                self.focused_id = Some(input_id.clone());
                state.was_focused = true;
            }
            let focused = self.focused_id.as_deref() == Some(input_id.as_str());
            if state.was_focused && !focused {
                state.editing = false;
                let next = state.buffer.trim().to_string();
                if next.is_empty() {
                    state.buffer.clone_from(text);
                    cancelled = true;
                } else {
                    if text.as_str() != next {
                        text.clone_from(&next);
                        changed = true;
                    }
                    state.buffer.clone_from(text);
                    submitted = true;
                }
                self.focused_id = Some(id.to_string());
            } else if self.key_pressed(Key::Escape) {
                state.editing = false;
                state.buffer.clone_from(text);
                cancelled = true;
                self.focused_id = Some(id.to_string());
            } else if self.key_pressed(Key::Enter) {
                let next = state.buffer.trim().to_string();
                state.editing = false;
                if next.is_empty() {
                    state.buffer.clone_from(text);
                    cancelled = true;
                } else {
                    if text.as_str() != next {
                        text.clone_from(&next);
                        changed = true;
                    }
                    state.buffer.clone_from(text);
                    submitted = true;
                }
                self.focused_id = Some(id.to_string());
            }
        }
        if state.editing {
            let field = self.text_field_impl(&input_id, &mut state.buffer, None, false, None, false);
            if state.was_focused && !field.focused {
                let next = state.buffer.trim().to_string();
                state.editing = false;
                if next.is_empty() {
                    state.buffer.clone_from(text);
                    cancelled = true;
                } else {
                    if text.as_str() != next {
                        text.clone_from(&next);
                        changed = true;
                    }
                    state.buffer.clone_from(text);
                    submitted = true;
                }
                self.focused_id = Some(id.to_string());
            } else {
                state.was_focused = field.focused;
            }
        }
        let focused = if state.editing {
            self.focused_id.as_deref() == Some(input_id.as_str())
        } else {
            self.focused_id.as_deref() == Some(id)
        };
        if !state.editing {
            // At rest MyGo shows the name as plain text, as wide as it is
            // (`ui/editable.go` coreEditableText).
            let height = measured_text_height(theme.font_sizes.body);
            let text_width = measure_text_width(text, theme.font_sizes.body).max(1.0);
            let node = self.new_named_node(
                id,
                Style {
                    width: Some(text_width),
                    height: Some(height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, id.to_string());
            self.register_focusable(node, id.to_string());
            let commands = vec![DrawCommand::Text {
                x: 0.0,
                y: 0.0,
                text: text.to_string(),
                font_size: theme.font_sizes.body,
                weight: 400,
                italic: false,
                mono: false,
                family: None,
                color: theme.foreground,
                max_width: Some(text_width),
                alignment: TextAlignment::Start,
            }];
            self.register_access_node(node, Role::Button, text, Some("editable"), focused);
            self.nodes
                .get_mut(&node)
                .expect("editable text exists")
                .commands = commands;
        }
        EditableTextResponse {
            changed,
            submitted,
            cancelled,
            editing: state.editing,
            focused,
        }
    }

    /// Creates a chip-style token field.
    pub fn token_field(
        &mut self,
        id: &str,
        tokens: &mut Vec<String>,
        input: &mut String,
    ) -> TokenFieldState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        let input_id = format!("{id}:input");
        let gap = 6.0;
        // MyGo's token field: a wrapping field of chips and the input
        // (`ui/combobox.go` coreTokenField). Its Space(1.5) of vertical and
        // Space(2.5) of horizontal padding sit inside a one-pixel border,
        // which takes layout space, so the content starts a pixel in and the
        // box is two pixels larger than the padding and a chip.
        let field = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Row,
                flex_wrap: true,
                gap,
                align_items: AlignItems::Center,
                padding: Edges {
                    top: 7.0,
                    right: 11.0,
                    bottom: 7.0,
                    left: 11.0,
                },
                ..Style::default()
            },
        );
        {
            let node_ref = self.nodes.get_mut(&field).expect("token field exists");
            node_ref.fill = Some((theme.surface, Radius::all(6.0)));
            node_ref.stroke = Some((theme.border, Radius::all(6.0)));
        }
        self.push_parent(field);
        let mut removed_index = None;
        for index in 0..tokens.len() {
            let token = tokens[index].clone();
            let key = format!("{id}:token:{token}");
            let remove_key = format!("{id}:token:{token}:remove");
            let text_width = measure_text_width(&token, font_size);
            // The chip gaps its text and remove button by Space(1).
            let chip_gap = 4.0;
            let chip_height = measured_text_height(font_size) + 2.0;
            let chip_width = text_width + 8.0 + chip_gap + 14.0 + 4.0;
            let chip = self.new_named_node(
                &key,
                Style {
                    width: Some(chip_width),
                    height: Some(chip_height),
                    flex_direction: FlexDirection::Row,
                    gap: chip_gap,
                    align_items: AlignItems::Center,
                    padding: Edges {
                        top: 1.0,
                        right: 4.0,
                        bottom: 1.0,
                        left: 8.0,
                    },
                    ..Style::default()
                },
            );
            {
                let node_ref = self.nodes.get_mut(&chip).expect("chip exists");
                node_ref.fill = Some((theme.surface_pressed, Radius::all(6.0)));
            }
            self.push_parent(chip);
            self.text(&token);
            let remove = self.new_named_node(
                &remove_key,
                Style {
                    width: Some(14.0),
                    height: Some(14.0),
                    ..Style::default()
                },
            );
            self.register_interaction(remove, remove_key.clone());
            let hovered = self.pointer_in_previous(&remove_key);
            let clicked = self.clicked_in_previous(&remove_key);
            if clicked {
                removed_index = Some(index);
            }
            let mut commands = Vec::new();
            if hovered {
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: 14.0,
                    height: 14.0,
                    radius: Radius::all(4.0),
                    color: theme.surface_hover,
                });
            }
            for (from, to) in [((4.2, 4.2), (9.8, 9.8)), ((9.8, 4.2), (4.2, 9.8))] {
                commands.push(DrawCommand::Line {
                    x1: from.0,
                    y1: from.1,
                    x2: to.0,
                    y2: to.1,
                    width: 1.25,
                    color: theme.muted,
                });
            }
            self.register_access_node(
                remove,
                Role::Button,
                &format!("remove token {}", token),
                None,
                false,
            );
            self.nodes
                .get_mut(&remove)
                .expect("token remove exists")
                .commands = commands;
            self.pop_parent();
        }
        let mut field_state = None;
        self.container().width(80.0).children(|context| {
            field_state = Some(context.text_field_bare(&input_id, input));
        });
        self.pop_parent();
        let field_state = field_state.expect("token input exists");
        let mut added = false;
        let mut removed = false;
        if let Some(index) = removed_index {
            tokens.remove(index);
            removed = true;
            self.focused_id = Some(input_id.clone());
        }
        let mut typed = input.clone();
        if field_state.focused {
            while let Some(position) = typed.find(',') {
                let token = typed[..position].trim().to_string();
                if !token.is_empty() && !tokens.contains(&token) {
                    tokens.push(token);
                    added = true;
                }
                typed.drain(..=position);
            }
            if self.key_pressed(Key::Enter) {
                let token = typed.trim().to_string();
                if !token.is_empty() && !tokens.contains(&token) {
                    tokens.push(token);
                    typed.clear();
                    added = true;
                }
            }
            if typed.is_empty() && self.key_pressed(Key::Backspace) && !tokens.is_empty() {
                tokens.pop();
                removed = true;
            }
        }
        *input = typed;
        TokenFieldState {
            changed: field_state.changed || added || removed,
            added,
            removed,
            focused: field_state.focused,
            hovered: field_state.hovered,
        }
    }

    /// Creates a text input with caller-provided prefix suggestions.
    pub fn autocomplete(
        &mut self,
        id: &str,
        text: &mut String,
        state: &mut AutocompleteState,
        candidates: &[&str],
    ) -> AutocompleteResponse {
        let theme = self.theme.clone();
        let input_id = format!("{id}:input");
        let popup_id = format!("{id}:popup");
        let input_bounds = self
            .previous_regions
            .get(&input_id)
            .map(|region| region.bounds);
        let focused = self.focused_id.as_deref() == Some(input_id.as_str());
        let previous_text = text.clone();
        let previous_selected = state.selected;
        let filtered = filter_suggestions(candidates, text);
        let selected_matches = state
            .selected
            .and_then(|index| candidates.get(index))
            .is_some_and(|candidate| candidate == text);
        let can_show = focused
            && !text.trim().is_empty()
            && !selected_matches
            && state.dismissed.as_deref() != Some(text.as_str())
            && !filtered.is_empty();
        let mut selected_now = false;
        if can_show {
            if !filtered.is_empty() {
                if self.key_pressed(Key::ArrowDown) {
                    state.highlight = (state.highlight + 1).min(filtered.len() - 1);
                }
                if self.key_pressed(Key::ArrowUp) {
                    state.highlight = state.highlight.saturating_sub(1);
                }
            }
            if self.key_pressed(Key::Enter) && !filtered.is_empty() {
                let original = filtered[state.highlight];
                text.clear();
                text.push_str(candidates[original]);
                state.selected = Some(original);
                state.dismissed = Some(candidates[original].to_string());
                selected_now = true;
                self.close_overlay_runtime(&popup_id);
                self.focused_id = Some(input_id.clone());
            }
        }
        // MyGo's autocomplete input is a field like the combobox's, without
        // the chevron (`ui/combobox.go` coreAutocomplete).
        let font_size = self.theme.font_sizes.body;
        let field_width = self.current_content_width();
        let field_height = measured_text_height(font_size) + 14.0;
        let inner_width = (field_width - 22.0).max(40.0);
        let field_node = self.new_named_node(
            &format!("{id}:field"),
            Style {
                width: Some(field_width),
                height: Some(field_height),
                flex_direction: FlexDirection::Row,
                gap: 6.0,
                align_items: AlignItems::Center,
                // MyGo's input places its text after a 1 DIP border and 10
                // DIP padding on each side (`ui/layout.go` contentX).
                padding: Edges::new(6.0, 11.0, 6.0, 11.0),
                ..Style::default()
            },
        );
        {
            let node_ref = self.nodes.get_mut(&field_node).expect("autocomplete field exists");
            node_ref.fill = Some((theme.surface, Radius::all(6.0)));
            node_ref.stroke = Some((theme.border, Radius::all(6.0)));
        }
        self.push_parent(field_node);
        let mut field_state = None;
        self.container().width(inner_width).children(|context| {
            field_state = Some(context.text_field_bare_placeholder(
                &input_id,
                text,
                state.placeholder.as_deref(),
            ));
        });
        self.pop_parent();
        let field = field_state.expect("autocomplete input exists");
        if field.changed {
            state.selected = None;
            state.dismissed = None;
            state.highlight = 0;
        }
        let filtered = filter_suggestions(candidates, text);
        let selected_matches = state
            .selected
            .and_then(|index| candidates.get(index))
            .is_some_and(|candidate| candidate == text);
        let mut visible = field.focused
            && !text.trim().is_empty()
            && !selected_matches
            && state.dismissed.as_deref() != Some(text.as_str())
            && !filtered.is_empty();
        if visible {
            state.highlight = state.highlight.min(filtered.len().saturating_sub(1));
        }
        let mut clicked = None;
        if visible {
            if let Some(anchor) = input_bounds {
                let highlight = state.highlight;
                let selected = state.selected;
                let mut overlay_state = OverlayState::new(true);
                let response = self.overlay(&popup_id, &mut overlay_state, anchor, |context| {
                    for (filtered_index, original) in filtered.iter().enumerate() {
                        let key = format!("{id}:option:{filtered_index}");
                        if context.select_option(
                            &key,
                            candidates[*original],
                            filtered_index == highlight,
                            selected == Some(*original),
                        ) {
                            clicked = Some(*original);
                        }
                    }
                });
                if !response.open {
                    state.dismissed = Some(text.clone());
                    visible = false;
                }
                self.focused_id = Some(input_id.clone());
            }
        }
        if let Some(original) = clicked {
            text.clear();
            text.push_str(candidates[original]);
            state.selected = Some(original);
            state.dismissed = Some(candidates[original].to_string());
            selected_now = true;
            visible = false;
            self.close_overlay_runtime(&popup_id);
            self.focused_id = Some(input_id.clone());
        }
        let changed = field.changed
            || selected_now
            || text != &previous_text
            || state.selected != previous_selected;
        AutocompleteResponse {
            selected: state.selected,
            changed,
            focused: field.focused,
            visible,
        }
    }

    /// Creates an editable combobox with a filtered overlay list.
    pub fn combobox(
        &mut self,
        id: &str,
        query: &mut String,
        state: &mut ComboboxState,
        options: &[&str],
    ) -> ComboboxResponse {
        let theme = self.theme.clone();
        let input_id = format!("{id}:input");
        let popup_id = format!("{id}:popup");
        let input_bounds = self
            .previous_regions
            .get(&input_id)
            .map(|region| region.bounds);
        let input_clicked = self.clicked_in_previous(&input_id);
        if input_clicked {
            state.open = true;
            state.highlight = 0;
        }
        let focused = self.focused_id.as_deref() == Some(input_id.as_str());
        let previous_selected = state.selected;
        let previous_query = query.clone();
        let filtered = filter_options(options, query);
        if focused
            && !state.open
            && (self.key_pressed(Key::ArrowDown) || self.key_pressed(Key::ArrowUp))
        {
            state.open = true;
            state.highlight = 0;
        }
        let mut selected_now = false;
        if state.open && focused {
            if self.key_pressed(Key::Escape) {
                state.open = false;
                self.close_overlay_runtime(&popup_id);
            } else {
                if !filtered.is_empty() {
                    if self.key_pressed(Key::ArrowDown) {
                        state.highlight = (state.highlight + 1).min(filtered.len() - 1);
                    }
                    if self.key_pressed(Key::ArrowUp) {
                        state.highlight = state.highlight.saturating_sub(1);
                    }
                }
                if self.key_pressed(Key::Enter) && !filtered.is_empty() {
                    let original = filtered[state.highlight];
                    query.clear();
                    query.push_str(options[original]);
                    state.selected = Some(original);
                    state.open = false;
                    selected_now = true;
                    self.close_overlay_runtime(&popup_id);
                    self.focused_id = Some(input_id.clone());
                }
            }
        }
        // MyGo's combobox is a field: Surface with a Border, the input
        // inside it and a 10px chevron at its end (`ui/combobox.go`).
        let font_size = self.theme.font_sizes.body;
        let field_width = self.current_content_width();
        let field_height = measured_text_height(font_size) + 14.0;
        let chevron_size = 10.0;
        let field_gap = 6.0;
        let inner_width = (field_width - 20.0 - field_gap - chevron_size - 2.0).max(40.0);
        // The field's one-pixel border takes layout room, so its content
        // starts a pixel inside the padding (`ui/combobox.go` field).
        let field_node = self.new_named_node(
            &format!("{id}:field"),
            Style {
                width: Some(field_width),
                height: Some(field_height),
                flex_direction: FlexDirection::Row,
                gap: field_gap,
                align_items: AlignItems::Center,
                padding: Edges::symmetric(7.0, 11.0),
                ..Style::default()
            },
        );
        {
            let node_ref = self.nodes.get_mut(&field_node).expect("combobox field exists");
            node_ref.fill = Some((theme.surface, Radius::all(6.0)));
            node_ref.stroke = Some((theme.border, Radius::all(6.0)));
        }
        self.push_parent(field_node);
        let mut field_state = None;
        self.container().width(inner_width).children(|context| {
            field_state = Some(context.text_field_bare(&input_id, query));
        });
        self.icon(&format!("{id}:chevron"), "chevron_down", chevron_size, theme.muted);
        self.pop_parent();
        let field = field_state.expect("combobox input exists");
        if field.changed {
            state.selected = None;
            if state.open {
                state.highlight = 0;
            }
        }
        let filtered = filter_options(options, query);
        if state.open {
            state.highlight = state.highlight.min(filtered.len().saturating_sub(1));
        }
        let mut clicked_option = None;
        if state.open {
            if let Some(anchor) = input_bounds {
                let highlight = state.highlight;
                let selected = state.selected;
                let mut overlay_state = OverlayState::new(true);
                let response = self.overlay(&popup_id, &mut overlay_state, anchor, |context| {
                    if filtered.is_empty() {
                        context
                            .text("No matches")
                            .font_size(theme.font_sizes.small)
                            .color(theme.muted);
                        return;
                    }
                    for (filtered_index, original) in filtered.iter().enumerate() {
                        let key = format!("{id}:option:{filtered_index}");
                        if context.select_option(
                            &key,
                            options[*original],
                            filtered_index == highlight,
                            selected == Some(*original),
                        ) {
                            clicked_option = Some(*original);
                        }
                    }
                });
                if !response.open {
                    state.open = false;
                }
                self.focused_id = Some(input_id.clone());
            }
        }
        if let Some(original) = clicked_option {
            query.clear();
            query.push_str(options[original]);
            state.selected = Some(original);
            state.open = false;
            selected_now = true;
            self.close_overlay_runtime(&popup_id);
            self.focused_id = Some(input_id.clone());
        }
        let changed = field.changed
            || selected_now
            || state.selected != previous_selected
            || query.as_str() != previous_query;
        ComboboxResponse {
            open: state.open,
            selected: state.selected,
            highlight: state.highlight,
            changed,
            focused: field.focused,
        }
    }

    /// Draws decoded RGBA8 pixels scaled into a target rectangle.
    pub fn image(
        &mut self,
        id: &str,
        data: ImageData<'_>,
        target_width: f32,
        target_height: f32,
    ) -> ImageState {
        self.image_styled(id, data, target_width, target_height, ImageStyle::default())
    }

    /// Draws an image with MyGo's Fit, corner radius and grayscale.
    pub fn image_styled(
        &mut self,
        id: &str,
        data: ImageData<'_>,
        target_width: f32,
        target_height: f32,
        style: ImageStyle,
    ) -> ImageState {
        let width = if target_width.is_finite() && target_width > 0.0 {
            target_width.round() as u32
        } else {
            0
        };
        let height = if target_height.is_finite() && target_height > 0.0 {
            target_height.round() as u32
        } else {
            0
        };
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width as f32),
                height: Some(height as f32),
                ..Style::default()
            },
        );
        let mut commands = Vec::new();
        let (columns, rows) = if data.is_valid() && width > 0 && height > 0 {
            // MyGo's fitIn (`ui/paint.go`): the picture is scaled into the
            // box as Fit says, and only the part that falls inside it shows.
            let box_width = width as f32;
            let box_height = height as f32;
            let source_width = data.width as f32;
            let source_height = data.height as f32;
            // MyGo's Fit fills the box whatever the picture's shape
            // (`ui/paint.go` fitIn's `case FillBox: return dst, src`), and
            // scales the rest about the box's middle.
            let scale = match style.fit {
                ImageFit::Fill => 1.0,
                ImageFit::Contain => (box_width / source_width).min(box_height / source_height),
                ImageFit::Cover => (box_width / source_width).max(box_height / source_height),
                ImageFit::ScaleDown => (box_width / source_width)
                    .min(box_height / source_height)
                    .min(1.0),
                ImageFit::NaturalSize => 1.0,
            };
            let (placed_width, placed_height) = if style.fit == ImageFit::Fill {
                (box_width, box_height)
            } else {
                (source_width * scale, source_height * scale)
            };
            let placed_x = (box_width - placed_width) / 2.0;
            let placed_y = (box_height - placed_height) / 2.0;
            // MyGo's fitIn takes the source fractions from the unrounded
            // shown rect and only then snaps that rect to device pixels
            // (`ui/paint.go`): the sampler still walks the whole fraction
            // across the snapped box. Rounding the rect before taking the
            // fractions would shift every sample by the rounding error.
            let shown_left = placed_x.max(0.0);
            let shown_top = placed_y.max(0.0);
            let shown_right = (placed_x + placed_width).min(box_width);
            let shown_bottom = (placed_y + placed_height).min(box_height);
            let source_x = (shown_left - placed_x) / placed_width.max(1.0);
            let source_y = (shown_top - placed_y) / placed_height.max(1.0);
            let source_fraction_width = (shown_right - shown_left) / placed_width.max(1.0);
            let source_fraction_height = (shown_bottom - shown_top) / placed_height.max(1.0);
            let pixel_scale = self.pixel_scale;
            let snap = |value: f32| (value * pixel_scale).round() / pixel_scale;
            let shown_x = snap(shown_left);
            let shown_y = snap(shown_top);
            let shown_width = snap(shown_right) - shown_x;
            let shown_height = snap(shown_bottom) - shown_y;
            if shown_width > 0.0 && shown_height > 0.0 {
                let mut placed = Vec::new();
                scaled_rgba_commands_with(
                    data,
                    &mut placed,
                    shown_width,
                    shown_height,
                    ImageMask {
                        radius: Radius::all(style.radius),
                        origin_x: shown_x - placed_x,
                        origin_y: shown_y - placed_y,
                        width: placed_width,
                        height: placed_height,
                    },
                    ImageRegion {
                        x: source_x,
                        y: source_y,
                        width: source_fraction_width,
                        height: source_fraction_height,
                    },
                    style.grayscale,
                    self.pixel_scale,
                );
                for command in placed {
                    match command {
                        DrawCommand::Rect {
                            x,
                            y,
                            width: cell_width,
                            height: cell_height,
                            color,
                        } => commands.push(DrawCommand::Rect {
                            x: x + shown_x,
                            y: y + shown_y,
                            width: cell_width,
                            height: cell_height,
                            color,
                        }),
                        other => commands.push(other),
                    }
                }
            }
            (shown_width.round() as usize, shown_height.round() as usize)
        } else {
            (0, 0)
        };
        self.nodes
            .get_mut(&node)
            .expect("image node exists")
            .commands = commands;
        self.register_access_node(node, Role::Label, id, Some("image"), false);
        ImageState {
            drawn: columns > 0 && rows > 0,
            width,
            height,
            columns,
            rows,
            source_width: data.width,
            source_height: data.height,
        }
    }

    /// Renders read-only formatted paragraphs from inline Markdown.
    pub fn rich_text(&mut self, id: &str, source: &str) -> RichTextResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.xs,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let mut span_count = 0;
        for paragraph in source.split("\n\n") {
            let text = paragraph.trim();
            if text.is_empty() {
                continue;
            }
            let spans = parse_inline(text);
            span_count += spans.len();
            self.inline_row(&spans, theme.font_sizes.body, false);
        }
        self.pop_parent();
        self.register_access_node(
            root,
            Role::Group,
            id,
            Some(format!("{span_count} spans").as_str()),
            false,
        );
        RichTextResponse { spans: span_count }
    }

    /// Parses and renders basic Markdown blocks.
    pub fn markdown(&mut self, id: &str, source: &str) -> MarkdownResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let blocks = parse_markdown(source);
        let body = theme.font_sizes.body;
        let block_spacing = body * 0.75;
        let heading_sizes = [1.6_f32, 1.35, 1.15, 1.0, 0.95, 0.9];
        let heading_weights = [700_u16, 650, 600, 600, 600, 600];
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let mut prev_heading = 0_u8;
        let mut first = true;
        for (index, block) in blocks.iter().enumerate() {
            let mut top = 0.0_f32;
            if !first {
                top = block_spacing;
                if prev_heading > 0 {
                    top = body * 0.55;
                }
                if let MarkdownBlock::Heading { level, .. } = block {
                    top = top.max(body * 1.4);
                    let _ = level;
                }
            }
            prev_heading = match block {
                MarkdownBlock::Heading { level, .. } => *level,
                _ => 0,
            };
            if top > 0.0 {
                let spacer = self.new_node(Style {
                    width: Some(1.0),
                    height: Some(top),
                    ..Style::default()
                });
                let _ = spacer;
            }
            match block {
                MarkdownBlock::Heading { level, spans } => {
                    let size = body * heading_sizes[(*level as usize - 1).min(5)];
                    let weight = heading_weights[(*level as usize - 1).min(5)];
                    let color = if *level == 6 {
                        theme.muted
                    } else {
                        theme.foreground
                    };
                    let runs = markdown_runs(spans, size, weight, color, &theme);
                    // MyGo's LineHeight multiplies the view's font size, not
                    // the span's: a heading sits in a body line box.
                    self.rich_text_spans_sized(
                        &format!("{id}:h{index}"),
                        &runs,
                        width,
                        body * 1.5,
                    );
                }
                MarkdownBlock::Paragraph(spans) => {
                    let runs = markdown_runs(spans, body, 400, theme.foreground, &theme);
                    self.rich_text_spans_sized(
                        &format!("{id}:p{index}"),
                        &runs,
                        width,
                        body * 1.5,
                    );
                }
                MarkdownBlock::UnorderedList(items) => {
                    self.markdown_list(
                        &format!("{id}:ul{index}"),
                        items.iter().map(|item| (None, item.as_slice())).collect::<Vec<_>>().as_slice(),
                        width,
                        body,
                        &theme,
                    );
                }
                MarkdownBlock::TaskList(items) => {
                    self.markdown_list(
                        &format!("{id}:ul{index}"),
                        items
                            .iter()
                            .map(|item| (Some(item.checked), item.spans.as_slice()))
                            .collect::<Vec<_>>()
                            .as_slice(),
                        width,
                        body,
                        &theme,
                    );
                }
                MarkdownBlock::OrderedList(items) => {
                    self.markdown_numbered_list(&format!("{id}:ol{index}"), items, width, body, &theme);
                }
                MarkdownBlock::Code { language, lines } => {
                    self.markdown_code_block(&format!("{id}:code{index}"), language.as_deref(), lines, width, body, &theme);
                }
                MarkdownBlock::Table {
                    headers,
                    rows,
                    aligns,
                } => {
                    self.markdown_table(&format!("{id}:table{index}"), headers, rows, aligns, width, body, &theme);
                }
            }
            first = false;
        }
        self.pop_parent();
        self.register_access_node(
            root,
            Role::Group,
            id,
            Some(format!("{} blocks", blocks.len()).as_str()),
            false,
        );
        MarkdownResponse {
            blocks: blocks.len(),
        }
    }

    fn markdown_list(
        &mut self,
        id: &str,
        items: &[(Option<bool>, &[InlineSpan])],
        width: f32,
        body: f32,
        theme: &Theme,
    ) {
        let line_height = body * 1.5;
        let marker_width = body * 1.6;
        for (index, (task, spans)) in items.iter().enumerate() {
            if index > 0 {
                // A tight list spaces its items by 0.25em.
                self.new_node(Style {
                    width: Some(1.0),
                    height: Some(body * 0.25),
                    ..Style::default()
                });
            }
            let row = self.new_named_node(
                &format!("{id}:i{index}"),
                Style {
                    width: Some(width),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexStart,
                    ..Style::default()
                },
            );
            self.push_parent(row);
            let marker = self.new_node(Style {
                width: Some(marker_width),
                height: Some(line_height),
                ..Style::default()
            });
            let mut commands = Vec::new();
            match task {
                None => {
                    // MyGo centers the bullet in a line less its own width
                    // (`ui/markdown/view.go` marker).
                    let bullet = "•";
                    let bullet_width = host_text_width_weighted(bullet, body, false, false, 400)
                        .unwrap_or_else(|| measure_text_width(bullet, body));
                    let left = ((marker_width - 7.0) - bullet_width) / 2.0;
                    commands.push(DrawCommand::Text {
                        x: left,
                        y: (line_height - measured_text_height(body)) / 2.0,
                        text: bullet.to_string(),
                        font_size: body,
                        weight: 400,
                        italic: false,
                        mono: false,
                        family: None,
                        color: theme.muted,
                        max_width: None,
                        alignment: TextAlignment::Start,
                    });
                }
                Some(checked) => {
                    // A 0.95em box, its radius a fifth, centered in the
                    // line the item starts with (`marker`).
                    let size = body * 0.95;
                    let radius = (size / 5.0).max(3.0);
                    let top = ((body * 1.5 * 1.2 - size) / 2.0 - 1.0).max(0.0);
                    if *checked {
                        commands.push(DrawCommand::RoundedRect {
                            x: 0.0,
                            y: top,
                            width: size,
                            height: size,
                            radius: Radius::all(radius),
                            color: theme.accent,
                        });
                        let stroke = (size / 9.0).max(1.5);
                        commands.push(DrawCommand::Line {
                            x1: size * 0.25,
                            y1: top + size * 0.52,
                            x2: size * 0.43,
                            y2: top + size * 0.7,
                            width: stroke,
                            color: crate::paint::Color::rgb(255, 255, 255),
                        });
                        commands.push(DrawCommand::Line {
                            x1: size * 0.43,
                            y1: top + size * 0.7,
                            x2: size * 0.76,
                            y2: top + size * 0.32,
                            width: stroke,
                            color: crate::paint::Color::rgb(255, 255, 255),
                        });
                    } else {
                        commands.push(DrawCommand::RoundedRect {
                            x: 0.0,
                            y: top,
                            width: size,
                            height: size,
                            radius: Radius::all(radius),
                            color: theme.muted,
                        });
                        commands.push(DrawCommand::RoundedRect {
                            x: 1.5,
                            y: top + 1.5,
                            width: size - 3.0,
                            height: size - 3.0,
                            radius: Radius::all((radius - 1.5).max(0.0)),
                            color: theme.background,
                        });
                    }
                }
            }
            self.nodes.get_mut(&marker).expect("marker exists").commands = commands;
            let runs = markdown_runs(spans, body, 400, theme.foreground, theme);
            self.rich_text_spans_sized(
                &format!("{id}:i{index}:text"),
                &runs,
                width - marker_width,
                line_height,
            );
            self.pop_parent();
        }
    }

    fn markdown_numbered_list(
        &mut self,
        id: &str,
        items: &[Vec<InlineSpan>],
        width: f32,
        body: f32,
        theme: &Theme,
    ) {
        let line_height = body * 1.5;
        let marker_width = body * 1.6;
        for (index, spans) in items.iter().enumerate() {
            if index > 0 {
                self.new_node(Style {
                    width: Some(1.0),
                    height: Some(body * 0.25),
                    ..Style::default()
                });
            }
            let row = self.new_named_node(
                &format!("{id}:i{index}"),
                Style {
                    width: Some(width),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexStart,
                    ..Style::default()
                },
            );
            self.push_parent(row);
            let marker = self.new_node(Style {
                width: Some(marker_width),
                height: Some(line_height),
                ..Style::default()
            });
            let label = format!("{}.", index + 1);
            let label_width = measure_text_width(&label, body);
            self.nodes.get_mut(&marker).expect("marker exists").commands =
                vec![DrawCommand::Text {
                    x: marker_width - 7.0 - label_width,
                    y: (line_height - measured_text_height(body)) / 2.0,
                    text: label,
                    font_size: body,
                    weight: 400,
                    italic: false,
                    mono: false,
                    family: None,
                    color: theme.muted,
                    max_width: None,
                    alignment: TextAlignment::Start,
                }];
            let runs = markdown_runs(spans, body, 400, theme.foreground, theme);
            self.rich_text_spans_sized(
                &format!("{id}:i{index}:text"),
                &runs,
                width - marker_width,
                line_height,
            );
            self.pop_parent();
        }
    }

    fn markdown_code_block(
        &mut self,
        id: &str,
        language: Option<&str>,
        lines: &[String],
        width: f32,
        body: f32,
        theme: &Theme,
    ) {
        let pad = body * 0.85;
        let code_size = body * 0.875;
        // The code's own line box, the monospace font's ascent and descent,
        // which the block centers in its taller `1.4` line height
        // (`internal/text/layout.go` line: Baseline sits half the leftover
        // above the ascent).
        let code_line = measured_text_height_styled(code_size, true);
        // The head's row is as tall as its tallest child, the copy button:
        // a `0.95em` icon and a `0.8em` label in `0.2em` of padding either
        // side. The label's line box, `0.8em` at the theme's 1.5 line
        // height, is taller than the icon, so the row is `1.6em`
        // (`ui/markdown/code.go` DefaultCodeBlock and CopyButton).
        let copy_content = (body * 0.95).max(body * 0.8 * 1.5);
        let button_height = copy_content + body * 0.2 * 2.0;
        let head_height = button_height.max(body * 0.8 * 1.5);
        let head_top = body * 0.35;
        let line_height = body * 0.875 * 1.4;
        let body_top = body * 0.25;
        let height = 2.0
            + head_top
            + head_height
            + body_top
            + line_height * lines.len() as f32
            + pad;
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.nodes.get_mut(&node).expect("code node exists").fill = Some((
            Color::rgba(24, 24, 27, 10),
            Radius::all(8.0),
        ));
        self.nodes.get_mut(&node).expect("code node exists").stroke =
            Some((theme.border, Radius::all(8.0)));
        // A Text command's y is the top of its line box, and the baseline
        // sits half the leftover above the ascent when the line is taller
        // than the font's own extent (`internal/text/layout.go` line:
        // `Baseline = y + (Height-Ascent-Descent)/2 + Ascent`). Without the
        // leftover the label draws about two pixels high.
        let label_size = body * 0.8;
        let label_line = label_size * 1.5;
        let label_shift = (label_line - measured_text_height_styled(label_size, false)) / 2.0;
        let mut commands = Vec::new();
        if let Some(language) = language {
            commands.push(DrawCommand::Text {
                x: 1.0 + body * 0.85,
                y: 1.0 + head_top + (head_height - label_line) / 2.0 + label_shift,
                text: language.to_string(),
                font_size: label_size,
                weight: 400,
                italic: false,
                mono: false,
                family: None,
                color: theme.muted,
                max_width: Some((width - body * 0.8).max(1.0)),
                alignment: TextAlignment::Start,
            });
        }
        // The copy button: an icon and its label, muted, at the right. MyGo
        // pads it `(0.2em, 0.45em)` with a `0.3em` gap, and its row centers
        // what it holds (`ui/markdown/code.go` CopyButton), so the button's
        // height is its tallest child, the `0.95em` icon.
        let copy_label = "Copy";
        let copy_size = label_size;
        let copy_width = measure_text_width(copy_label, copy_size);
        let icon_size = body * 0.95;
        let copy_pad_x = body * 0.45;
        let copy_pad_y = body * 0.2;
        let copy_gap = body * 0.3;
        let button_width = copy_pad_x * 2.0 + icon_size + copy_gap + copy_width;
        let button_x = width - 1.0 - body * 0.4 - button_width;
        let button_y = 1.0 + head_top + (head_height - button_height) / 2.0;
        // The copy glyph is a 16-unit view box, as MyGo's copyIcon is; the
        // painter scales it into its target box, so the path is mapped
        // rather than drawn at its own coordinates.
        let icon_x = button_x + copy_pad_x;
        // The row centers the icon inside the label's taller line box.
        let icon_y = button_y + copy_pad_y + (copy_content - icon_size) / 2.0;
        let icon_scale = icon_size / 16.0;
        commands.push(DrawCommand::TransformPush {
            transform: Transform {
                translation: Point::new(icon_x, icon_y),
                scale: Point::new(icon_scale, icon_scale),
                rotation: 0.0,
            },
        });
        commands.push(DrawCommand::SvgPath {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 16.0,
            path: "M7.1 5.5H11.9A1.6 1.6 0 0 1 13.5 7.1V11.9A1.6 1.6 0 0 1 11.9 13.5H7.1A1.6 1.6 0 0 1 5.5 11.9V7.1A1.6 1.6 0 0 1 7.1 5.5ZM10.5 3.6V3.2A1.7 1.7 0 0 0 8.8 1.5H4.2A1.7 1.7 0 0 0 2.5 3.2v4.6a1.7 1.7 0 0 0 1.7 1.7h.4".to_string(),
            stroke: Some(theme.muted),
            stroke_width: 1.3,
            fill: None,
        });
        commands.push(DrawCommand::TransformPop);
        commands.push(DrawCommand::Text {
            x: button_x + copy_pad_x + icon_size + copy_gap,
            y: button_y + (button_height - label_line) / 2.0 + label_shift,
            text: copy_label.to_string(),
            font_size: copy_size,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: theme.muted,
            max_width: Some(copy_width.max(1.0)),
            alignment: TextAlignment::Start,
        });
        for (index, line) in lines.iter().enumerate() {
            let top = 1.0 + head_top + head_height + body_top + line_height * index as f32;
            let y = top + (line_height - code_line) / 2.0;
            let mut x = 1.0 + pad;
            let runs: Vec<(String, CodeColor)> = if language
                .map(|name| matches!(name.to_ascii_lowercase().as_str(), "go" | "golang"))
                .unwrap_or(false)
            {
                go_tokens(line)
            } else {
                vec![(line.clone(), CodeColor::Text)]
            };
            for (text, kind) in runs {
                if text.is_empty() {
                    continue;
                }
                let run_width = host_text_width_weighted(&text, code_size, false, true, 400)
                    .unwrap_or_else(|| measure_text_width(&text, code_size));
                let color = match kind {
                    CodeColor::Text => theme.foreground,
                    CodeColor::Keyword | CodeColor::Operator => Color::rgb(207, 34, 46),
                    CodeColor::Type => Color::rgb(149, 56, 0),
                    CodeColor::Constant => Color::rgb(5, 80, 174),
                    CodeColor::Function => Color::rgb(130, 80, 223),
                    CodeColor::Property | CodeColor::Number => Color::rgb(5, 80, 174),
                    CodeColor::String => Color::rgb(10, 48, 105),
                    CodeColor::Comment => Color::rgb(110, 119, 129),
                };
                commands.push(DrawCommand::Text {
                    x,
                    y,
                    text,
                    font_size: code_size,
                    weight: 400,
                    italic: false,
                    mono: true,
                    family: None,
                    color,
                    max_width: Some(run_width.max(1.0)),
                    alignment: TextAlignment::Start,
                });
                x += run_width;
            }
        }
        self.nodes.get_mut(&node).expect("code node exists").commands = commands;
    }

    fn markdown_table(
        &mut self,
        id: &str,
        headers: &[String],
        rows: &[Vec<String>],
        aligns: &[TableAlign],
        width: f32,
        body: f32,
        theme: &Theme,
    ) {
        let pad_x = body * 0.6;
        let pad_y = body * 0.4;
        let line_height = body * 1.5;
        let row_height = line_height + pad_y * 2.0;
        // A body row carries a one-pixel rule above it, inside its box.
        let data_row_height = row_height + 1.0;
        let columns = headers
            .len()
            .max(rows.iter().map(Vec::len).max().unwrap_or(0))
            .max(1);
        let mut widths = vec![0.0_f32; columns];
        for (column, header) in headers.iter().enumerate() {
            let text_width = measure_text_width(header, body);
            widths[column] = widths[column].max((text_width).min(body * 24.0) + pad_x * 2.0 + 1.0);
        }
        for row in rows {
            for (column, cell) in row.iter().enumerate().take(columns) {
                let text_width = measure_text_width(cell, body);
                widths[column] = widths[column].max((text_width).min(body * 24.0) + pad_x * 2.0 + 1.0);
            }
        }
        let table_width: f32 = widths.iter().sum::<f32>() + 2.0;
        // The layout keeps the rows' fractional height, as MyGo's does, and
        // the painter rounds each edge of the frame to a device pixel
        // (`ui/paint.go` snap), so a frame that ends part way through a pixel
        // still paints a whole bottom rule.
        let height = 2.0 + row_height + data_row_height * rows.len() as f32;
        let node = self.new_named_node(
            id,
            Style {
                width: Some(table_width.min(width)),
                height: Some(height),
                ..Style::default()
            },
        );
        // MyGo fills the table's frame and its inner face, so every rule is
        // a whole device pixel; stroked rules blur across two at fractional
        // layout positions. The outer box carries the border color, and the
        // face one pixel in covers it everywhere but the rules. Both faces
        // are painted here rather than left to the node, whose stroke would
        // be a half-pixel-out path once the node's own offset applies.
        let radius = body * 0.3;
        let mut commands = Vec::new();
        commands.push(DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width: table_width.min(width),
            height,
            radius: Radius::all(radius),
            color: theme.border,
        });
        commands.push(DrawCommand::RoundedRect {
            x: 1.0,
            y: 1.0,
            width: (table_width.min(width) - 2.0).max(0.0),
            height: (height - 2.0).max(0.0),
            radius: Radius::all((radius - 1.0).max(0.0)),
            color: theme.background,
        });
        // Header face: 4% of the text color.
        commands.push(DrawCommand::Rect {
            x: 1.0,
            y: 1.0,
            width: table_width - 2.0,
            height: row_height,
            color: Color::rgba(24, 24, 27, 10),
        });
        for row_index in 1..=rows.len() {
            let y = 1.0 + row_height + data_row_height * (row_index - 1) as f32;
            commands.push(DrawCommand::Rect {
                x: 1.0,
                y,
                width: table_width - 2.0,
                height: 1.0,
                color: theme.border,
            });
        }
        let mut x = 1.0;
        for (column, column_width) in widths.iter().enumerate() {
            if column > 0 {
                commands.push(DrawCommand::Rect {
                    x,
                    y: 1.0,
                    width: 1.0,
                    height: height - 2.0,
                    color: theme.border,
                });
            }
            x += column_width;
        }
        let draw_cell = |commands: &mut Vec<DrawCommand>, text: &str, column: usize, cell_x: f32, top: f32, header: bool| {
            let base_weight = if header { 600 } else { 400 };
            let spans = parse_inline(text);
            let span_width = |span: &InlineSpan| -> f32 {
                let weight = if header || matches!(span.style, InlineStyle::Bold) {
                    600
                } else {
                    400
                };
                let mono = matches!(span.style, InlineStyle::Code);
                let size = if mono { body * 0.875 } else { body };
                host_text_width_weighted(&span.text, size, false, mono, weight)
                    .unwrap_or_else(|| measure_text_width(&span.text, size))
            };
            let text_width: f32 = spans.iter().map(span_width).sum();
            let align = aligns.get(column).copied().unwrap_or_default();
            let inner = (widths[column] - pad_x * 2.0).max(1.0);
            let start_x = match align {
                TableAlign::Start => cell_x + pad_x,
                TableAlign::Center => cell_x + pad_x + (inner - text_width).max(0.0) / 2.0,
                TableAlign::End => cell_x + widths[column] - pad_x - text_width,
            };
            let mut x = start_x;
            for span in &spans {
                let weight = if header || matches!(span.style, InlineStyle::Bold) {
                    600
                } else {
                    400
                };
                let mono = matches!(span.style, InlineStyle::Code);
                let italic = matches!(span.style, InlineStyle::Italic);
                let size = if mono { body * 0.875 } else { body };
                let color = match &span.style {
                    InlineStyle::Link { .. } => theme.accent,
                    _ => theme.foreground,
                };
                let width = span_width(span);
                commands.push(DrawCommand::Text {
                    x,
                    y: top
                        + pad_y
                        + (line_height - measured_text_height(body)) / 2.0
                        + (body - size) * 0.25,
                    text: span.text.clone(),
                    font_size: size,
                    weight,
                    italic,
                    mono,
                    family: None,
                    color,
                    max_width: Some(width.max(1.0)),
                    alignment: TextAlignment::Start,
                });
                x += width;
            }
            let _ = base_weight;
        };
        let mut cell_x = 1.0;
        for (column, header) in headers.iter().enumerate() {
            draw_cell(&mut commands, header, column, cell_x, 1.0, true);
            cell_x += widths[column];
        }
        for (row_index, row) in rows.iter().enumerate() {
            let mut cell_x = 1.0;
            let top = 1.0 + row_height + data_row_height * row_index as f32;
            for (column, cell) in row.iter().enumerate().take(columns) {
                draw_cell(&mut commands, cell, column, cell_x, top + 1.0, false);
                cell_x += widths[column];
            }
        }
        self.nodes.get_mut(&node).expect("table node exists").commands = commands;
    }

    /// Renders rich text spans that wrap across lines, as MyGo's
    /// `RichText` over `ui.Span`s.
    pub fn rich_text_spans(&mut self, id: &str, spans: &[RichTextSpan]) -> RichTextResponse {
        let width = self.current_content_width();
        self.rich_text_sized(id, spans, width, 0.0)
    }

    /// Draws runs as one wrapping paragraph inside `width`, each line box
    /// `line_height` tall with its glyphs centered, as MyGo's markdown
    /// body sets a line and a half.
    pub fn rich_text_spans_sized(
        &mut self,
        id: &str,
        spans: &[RichTextSpan],
        width: f32,
        line_height: f32,
    ) -> RichTextResponse {
        self.rich_text_sized(id, spans, width, line_height)
    }

    fn rich_text_sized(
        &mut self,
        id: &str,
        spans: &[RichTextSpan],
        width: f32,
        line_height_override: f32,
    ) -> RichTextResponse {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        // MyGo sets every run of a line on one baseline, the line's tallest
        // ascent (`internal/text` line()), and sizes the line box to the
        // body line height unless a run's ascent and descent need more.
        let body_ascent = host_text_ascent(font_size, false, false, false)
            .or_else(|| default_font().map(|font| font.ascent(font_size)))
            .unwrap_or(font_size * 0.93);
        let body_descent = host_text_descent(font_size, false, false, false)
            .unwrap_or(font_size * 0.25);
        let mut span_metrics = Vec::with_capacity(spans.len());
        for span in spans {
            let size = if span.font_size > 0.0 {
                span.font_size
            } else {
                font_size
            };
            let ascent = host_text_ascent(size, span.italic, span.mono, span.weight >= 600)
                .or_else(|| default_font().map(|font| font.ascent(size)))
                .unwrap_or(size * 0.93);
            let descent = host_text_descent(size, span.italic, span.mono, span.weight >= 600)
                .unwrap_or(size * 0.25);
            span_metrics.push((size, ascent, descent));
        }
        let line_ascent = span_metrics
            .iter()
            .fold(body_ascent, |ascent, metrics| ascent.max(metrics.1));
        let line_descent = span_metrics
            .iter()
            .fold(body_descent, |descent, metrics| descent.max(metrics.2));
        let line_height = if line_height_override > 0.0 {
            line_height_override.max(line_ascent + line_descent)
        } else {
            measured_text_height(font_size).max(line_ascent + line_descent)
        };
        // MyGo centers the line's faces in a line box taller than they are
        // (`internal/text` line()): its baseline drops by the surplus.
        let line_top = (line_height - line_ascent - line_descent) / 2.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Row,
                flex_wrap: true,
                align_items: AlignItems::FlexStart,
                ..Style::default()
            },
        );
        self.register_access_node(root, Role::Group, id, None, false);
        self.push_parent(root);
        for (span, &(size, ascent, _descent)) in spans.iter().zip(&span_metrics) {
            // This run's glyphs sit on the line baseline, not its own.
            let baseline = line_top + line_ascent - ascent;
            // Each run keeps a label node so assistive tech can read the
            // styled text, as the per-span layout used to.
            let span_value = if span.mono && span.background.is_some() {
                "code"
            } else if span.strikethrough {
                "strikethrough"
            } else if span.weight >= 600 {
                "bold"
            } else if span.italic {
                "italic"
            } else {
                "plain"
            };
            let mut span_registered = false;
            // A trailing space is advance, not ink: measure and reserve it
            // explicitly, since canvas measurement drops trailing spaces.
            let styled_width = |text: &str| {
                host_text_width_weighted(text, size, span.italic, span.mono, span.weight)
                    .unwrap_or_else(|| measure_text_width(text, size))
            };
            let space_width = (styled_width("x x") - styled_width("xx")).max(0.0);
            let trimmed = span.text.trim_end();
            let full_width = styled_width(trimmed);
            // A run that fits on one line stays one node, so a code pill or
            // a strikethrough covers the run whole rather than per word.
            let one_line = !span.text.trim_end().contains(' ');
            if !span.text.contains('\n')
                && full_width <= width
                && (span.strikethrough || span.background_radius > 0.0 || one_line)
            {
                let node = self.new_node(Style {
                    width: Some(full_width.max(1.0)),
                    height: Some(line_height),
                    ..Style::default()
                });
                let mut commands = Vec::new();
                let color = span.color.unwrap_or(theme.foreground);
                if let Some(background) = span.background {
                    if span.background_radius > 0.0 {
                        // MyGo hugs a code pill with a sixth of an em of
                        // side room and a line and a third of height
                        // (`ui/richtext.go` spanPaint.backgrounds).
                        let pad = (size * 0.22).min(span.background_radius);
                        commands.push(DrawCommand::RoundedRect {
                            x: -pad,
                            y: line_top + line_ascent - size * 0.98,
                            width: full_width + pad * 2.0,
                            height: size * 1.32,
                            radius: Radius::all(span.background_radius),
                            color: background,
                        });
                    } else {
                        commands.push(DrawCommand::Rect {
                            x: 0.0,
                            y: 0.0,
                            width: full_width,
                            height: line_height,
                            color: background,
                        });
                    }
                }
                commands.push(DrawCommand::Text {
                    x: 0.0,
                    y: baseline,
                    text: trimmed.to_string(),
                    font_size: size,
                    weight: span.weight,
                    italic: span.italic,
                    mono: span.mono,
                    family: None,
                    color,
                    max_width: Some(full_width.max(1.0)),
                    alignment: TextAlignment::Start,
                });
                if span.strikethrough {
                    // MyGo strikes through the middle of the lowercase: its
                    // canvas engine sets the strike's top half an x above the
                    // baseline, less half its own thickness
                    // (`internal/text/canvas_js.go` font, decorate).
                    let strike_top = host_text_x_height(size, span.italic, span.mono, span.weight >= 600)
                        .unwrap_or(size * 0.53) / 2.0
                        + strike_thickness(size) / 2.0;
                    let y = line_top + line_ascent - strike_top + strike_thickness(size) / 2.0;
                    commands.push(DrawCommand::Line {
                        x1: 0.0,
                        y1: y,
                        x2: full_width,
                        y2: y,
                        width: strike_thickness(size),
                        color,
                    });
                }
                self.nodes
                    .get_mut(&node)
                    .expect("rich text run exists")
                    .commands = commands;
                self.register_access_node(node, Role::Label, span.text.trim(), Some(span_value), false);
                continue;
            }
            for word in span.text.split_inclusive(' ') {
                let text = word.trim_end();
                // A run that is only space still advances: the space belongs
                // to this run's face, and dropping it glues the neighbouring
                // runs together.
                let mut word_width = if text.is_empty() {
                    0.0
                } else {
                    styled_width(text)
                };
                if text.len() != word.len() {
                    word_width += space_width;
                }
                if word_width <= 0.0 {
                    continue;
                }
                let node = self.new_node(Style {
                    width: Some(word_width.max(1.0)),
                    height: Some(line_height),
                    ..Style::default()
                });
                let mut commands = Vec::new();
                if let Some(background) = span.background {
                    if span.background_radius > 0.0 {
                        let pad = (size * 0.22).min(span.background_radius);
                        commands.push(DrawCommand::RoundedRect {
                            x: -pad,
                            y: line_top + line_ascent - size * 0.98,
                            width: word_width + pad * 2.0,
                            height: size * 1.32,
                            radius: Radius::all(span.background_radius),
                            color: background,
                        });
                    } else {
                        commands.push(DrawCommand::Rect {
                            x: 0.0,
                            y: 0.0,
                            width: word_width,
                            height: line_height,
                            color: background,
                        });
                    }
                }
                let color = span.color.unwrap_or(theme.foreground);
                commands.push(DrawCommand::Text {
                    x: 0.0,
                    y: baseline,
                    text: text.to_string(),
                    font_size: size,
                    weight: span.weight,
                    italic: span.italic,
                    mono: span.mono,
                    family: None,
                    color,
                    max_width: Some(word_width.max(1.0)),
                    alignment: TextAlignment::Start,
                });
                if span.strikethrough {
                    let strike_top = host_text_x_height(size, span.italic, span.mono, span.weight >= 600)
                        .unwrap_or(size * 0.53) / 2.0
                        + strike_thickness(size) / 2.0;
                    let y = line_top + line_ascent - strike_top + strike_thickness(size) / 2.0;
                    commands.push(DrawCommand::Line {
                        x1: 0.0,
                        y1: y,
                        x2: word_width,
                        y2: y,
                        width: strike_thickness(size),
                        color,
                    });
                }
                self.nodes
                    .get_mut(&node)
                    .expect("rich text word exists")
                    .commands = commands;
                if !span_registered {
                    span_registered = true;
                    self.register_access_node(
                        node,
                        Role::Label,
                        span.text.trim(),
                        Some(span_value),
                        false,
                    );
                }
            }
        }
        self.pop_parent();
        RichTextResponse { spans: spans.len() }
    }

    fn inline_row(&mut self, spans: &[InlineSpan], font_size: f32, heading: bool) {
        self.row().gap(0.0).wrap().children(|context| {
            for span in spans {
                context.inline_span_node(span, font_size, heading);
            }
        });
    }

    fn inline_span_node(&mut self, span: &InlineSpan, font_size: f32, heading: bool) -> u64 {
        let theme = self.theme.clone();
        let text_width = measure_text_width(&span.text, font_size);
        let text_height = measured_text_height(font_size);
        let (width, height) = match span.style {
            InlineStyle::Code => (text_width + 8.0, text_height + 2.0),
            _ => (text_width.max(1.0), text_height.max(1.0)),
        };
        let node = self.new_node(Style {
            width: Some(width),
            height: Some(height),
            ..Style::default()
        });
        let mut commands = Vec::new();
        let mut weight = if heading || matches!(span.style, InlineStyle::Bold) {
            700
        } else {
            400
        };
        let mut color = theme.foreground;
        let mut access_value = "plain";
        match &span.style {
            InlineStyle::Plain => {}
            InlineStyle::Bold => access_value = "bold",
            InlineStyle::Italic => {
                access_value = "italic";
                commands.push(DrawCommand::TransformPush {
                    transform: Transform {
                        rotation: 0.08,
                        ..Transform::default()
                    },
                });
            }
            InlineStyle::Code => {
                access_value = "code";
                weight = 600;
                color = theme.accent;
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                    radius: Radius::all(3.0),
                    color: mix_color(theme.surface, theme.foreground, 0.1),
                });
            }
            InlineStyle::Link { .. } => {
                access_value = "link";
                weight = 500;
                color = theme.accent;
            }
            InlineStyle::Strikethrough => {
                access_value = "strikethrough";
            }
        }
        let (text_x, text_y) = if matches!(span.style, InlineStyle::Code) {
            (4.0, 1.0)
        } else {
            (0.0, 0.0)
        };
        commands.push(DrawCommand::Text {
            x: text_x,
            y: text_y,
            text: span.text.clone(),
            font_size,
            weight,
            italic: matches!(span.style, InlineStyle::Italic),
            mono: matches!(span.style, InlineStyle::Code),
            family: None,
            color,
            max_width: Some(width.max(1.0)),
            alignment: TextAlignment::Start,
        });
        if matches!(span.style, InlineStyle::Italic) {
            commands.push(DrawCommand::TransformPop);
        }
        if matches!(span.style, InlineStyle::Strikethrough) {
            commands.push(DrawCommand::Line {
                x1: 0.0,
                y1: text_height * 0.62,
                x2: width,
                y2: text_height * 0.62,
                width: 1.0,
                color: theme.foreground,
            });
        }
        if matches!(span.style, InlineStyle::Link { .. }) {
            commands.push(DrawCommand::Line {
                x1: 0.0,
                y1: height - 1.0,
                x2: width,
                y2: height - 1.0,
                width: 1.0,
                color: theme.accent,
            });
        }
        self.register_access_node(node, Role::Label, &span.text, Some(access_value), false);
        self.nodes
            .get_mut(&node)
            .expect("inline span node exists")
            .commands = commands;
        node
    }

    /// Creates a find bar with wrapped previous/next navigation.
    pub fn find_bar(
        &mut self,
        id: &str,
        query: &mut String,
        current: &mut usize,
        matches: usize,
    ) -> FindBarState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        let input_id = format!("{id}:input");
        let prev_key = format!("{id}:prev");
        let next_key = format!("{id}:next");
        // A bar that was not here last frame takes the focus, its field's
        // text chosen ready to type over (`ui/feedback.go` coreFindBar).
        let opening = !self.previous_regions.contains_key(&input_id);
        if opening {
            self.focused_id = Some(input_id.clone());
        }
        if matches == 0 {
            *current = 0;
        } else {
            *current = (*current).min(matches - 1);
        }
        let previous_clicked = matches > 0 && self.clicked_in_previous(&prev_key);
        let next_clicked = matches > 0 && self.clicked_in_previous(&next_key);
        if previous_clicked {
            *current = if *current == 0 {
                matches - 1
            } else {
                *current - 1
            };
        }
        if next_clicked {
            *current = (*current + 1) % matches;
        }
        // MyGo's find bar: a Surface row with a bottom border, Space(2) of
        // gap and Space(1.5)/Space(3) padding (`ui/feedback.go` coreFindBar).
        let bar = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Row,
                gap: 8.0,
                align_items: AlignItems::Center,
                // MyGo's one-pixel bottom border takes layout room, so the
                // bar is a pixel taller than its padding and its line.
                padding: Edges {
                    top: 6.0,
                    right: 12.0,
                    bottom: 7.0,
                    left: 12.0,
                },
                ..Style::default()
            },
        );
        {
            let node_ref = self.nodes.get_mut(&bar).expect("find bar exists");
            node_ref.fill = Some((theme.surface, Radius::all(0.0)));
        }
        self.push_parent(bar);
        // MyGo's buttons carry their one-pixel border inside the padding:
        // the step buttons hold a 12×12 box padded 6/8, and Done is its
        // label padded 6/14, each two pixels more with the border.
        let status_width = 72.0;
        let step_width = 30.0;
        let step_height = 26.0;
        let done_text_width = measure_text_width("Done", font_size);
        let done_height = measured_text_height(font_size) + 14.0;
        let done_width = done_text_width + 30.0;
        let field_width = (width
            - 24.0
            - status_width
            - step_width * 2.0
            - done_width
            - 8.0 * 4.0)
            .max(120.0);
        // MyGo's field takes Space(1.5) of padding and Space(2.5) of side
        // padding inside its one-pixel border, which takes layout room, so
        // the content starts a pixel further in (`ui/combobox.go` field).
        let field = self.new_named_node(
            &format!("{id}:field"),
            Style {
                width: Some(field_width),
                height: Some(measured_text_height(font_size) + 14.0),
                flex_direction: FlexDirection::Row,
                gap: 6.0,
                align_items: AlignItems::Center,
                padding: Edges::symmetric(7.0, 11.0),
                ..Style::default()
            },
        );
        {
            let node_ref = self.nodes.get_mut(&field).expect("find field exists");
            node_ref.fill = Some((theme.surface, Radius::all(6.0)));
            node_ref.stroke = Some((theme.border, Radius::all(6.0)));
        }
        self.push_parent(field);
        // MyGo's magnifier is an icon Space(3.75) = 15 across
        // (`ui/combobox.go` magnifier), which its row centers.
        self.icon(&format!("{id}:icon"), "magnifier", 15.0, theme.muted);
        let mut input = None;
        let input_width = (field_width - 20.0 - 6.0 - 15.0 - 2.0).max(60.0);
        self.container().width(input_width).children(|context| {
            input = Some(context.text_field_bare(&input_id, query));
        });
        self.pop_parent();
        let input = input.expect("find input exists");
        if opening && !query.is_empty() {
            self.set_text_input_selection(&input_id, 0, query.len());
        }
        if input.focused {
            // As MyGo's field, the box's border turns the accent color while
            // its input owns the focus (`ui/form.go` inputBorder).
            self.nodes
                .get_mut(&field)
                .expect("find field exists")
                .stroke = Some((theme.accent, Radius::all(6.0)));
        }
        // The match count, in the muted color, Space(18) wide at least.
        let status = if query.is_empty() {
            String::new()
        } else if matches > 0 {
            format!("{} of {}", *current + 1, matches)
        } else {
            "No matches".to_string()
        };
        let muted = theme.muted;
        self.container().width(status_width).children(|context| {
            context.text(&status).font_size(theme.font_sizes.body).color(muted);
        });
        // Previous and next step buttons, arrows drawn as MyGo draws them.
        for (key, up) in [(prev_key.clone(), true), (next_key.clone(), false)] {
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(step_width),
                    height: Some(step_height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            let hovered = matches > 0 && self.pointer_in_previous(&key);
            let arrow = if up {
                "M3 7.5 L6 4.5 L9 7.5"
            } else {
                "M3 4.5 L6 7.5 L9 4.5"
            };
            let commands = vec![
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: step_width,
                    height: step_height,
                    radius: Radius::all(6.0),
                    color: theme.border,
                },
                DrawCommand::RoundedRect {
                    x: 1.0,
                    y: 1.0,
                    width: step_width - 2.0,
                    height: step_height - 2.0,
                    radius: Radius::all(5.0),
                    color: if hovered {
                        theme.surface_hover
                    } else {
                        theme.surface
                    },
                },
                DrawCommand::TransformPush {
                    transform: Transform::translate(Point::new(9.0, 7.0)),
                },
                DrawCommand::SvgPath {
                    x: 0.0,
                    y: 0.0,
                    width: 12.0,
                    height: 12.0,
                    path: arrow.to_string(),
                    fill: None,
                    stroke: Some(theme.foreground),
                    stroke_width: 1.5,
                },
                DrawCommand::TransformPop,
            ];
            self.nodes
                .get_mut(&node)
                .expect("find step exists")
                .commands = commands;
        }
        // Done, styled like MyGo's default button: its label padded 6/14.
        let done_id = format!("{id}:done");
        let done = self.new_named_node(
            &done_id,
            Style {
                width: Some(done_width),
                height: Some(done_height),
                ..Style::default()
            },
        );
        self.register_interaction(done, done_id.clone());
        let done_hovered = self.pointer_in_previous(&done_id);
        let done_pressed = done_hovered && self.input.pointer_down;
        let closed = self.clicked_in_previous(&done_id);
        let done_commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: done_width,
                height: done_height,
                radius: Radius::all(6.0),
                color: theme.border,
            },
            DrawCommand::RoundedRect {
                x: 1.0,
                y: 1.0,
                width: done_width - 2.0,
                height: done_height - 2.0,
                radius: Radius::all(5.0),
                color: if done_pressed {
                    theme.surface_pressed
                } else if done_hovered {
                    theme.surface_hover
                } else {
                    theme.surface
                },
            },
            DrawCommand::Text {
                x: 15.0,
                y: 7.0,
                text: "Done".to_string(),
                font_size,
                weight: 400,
                italic: false,
                mono: false,
                family: None,
                color: theme.foreground,
                max_width: Some(done_text_width),
                alignment: TextAlignment::Start,
            },
        ];
        self.nodes
            .get_mut(&done)
            .expect("find done exists")
            .commands = done_commands;
        self.pop_parent();
        // MyGo's `BorderWidth(0, 0, 1, 0)` fills the bar's bottom row with
        // the border color; a stroked line would center on the box edge and
        // spread across two half-covered rows instead.
        self.set_node_bottom_border(bar, theme.border);
        let changed = input.changed || previous_clicked || next_clicked;
        FindBarState {
            changed,
            current: *current,
            matches,
            query: query.clone(),
            previous_clicked,
            next_clicked,
            closed,
            focused: input.focused,
            hovered: self.pointer_in_previous(&input_id),
        }
    }

    /// Renders the current page from a caller-owned router.
    pub fn router(
        &mut self,
        id: &str,
        state: &mut RouterState,
        pages: &[Page<'_>],
    ) -> RouterResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let page = state.current().to_string();
        let page_closure = pages
            .iter()
            .find(|entry| entry.id == page.as_str())
            .map(|entry| entry.render);
        if let Some(render) = page_closure {
            render(self);
        } else {
            self.text("Page not found")
                .font_size(theme.font_sizes.small)
                .color(theme.muted);
        }
        self.pop_parent();
        self.register_access_node(root, Role::Group, &page, None, false);
        RouterResponse {
            page,
            rendered: page_closure.is_some(),
            can_back: state.can_back(),
            can_forward: state.can_forward(),
        }
    }

    /// Creates back/forward buttons driven by router history.
    pub fn back_forward_buttons(&mut self, id: &str, state: &mut RouterState) -> BackForwardState {
        let theme = self.theme.clone();
        let space = theme.spacing.base;
        let width = self.current_content_width();
        // MyGo's history buttons are buttons in a Toolbar, which strips
        // their face and border and pads them `Space(1.5), Space(2.5)`
        // (`ui/widgets.go` styleButton's toolbarButtons case); historyButton
        // then *replaces* that padding with `Space(1.5), Space(2)`
        // (`ui/router.go`), since `Padding` assigns rather than adds. Each
        // button holds a `Space(4)` box with a 1.6-wide chevron in it.
        let vertical = space * 1.5;
        let horizontal = space * 2.0;
        let arrow = space * 4.0;
        let height = arrow + vertical * 2.0;
        let button_width = arrow + horizontal * 2.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                // The Toolbar holding them gaps its children `Space(1)`.
                gap: space,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let back_key = format!("{id}:back");
        let forward_key = format!("{id}:forward");
        let mut back_clicked = false;
        let mut forward_clicked = false;
        for (key, name, direction, enabled) in [
            (back_key.as_str(), "back", -1.0_f32, state.can_back()),
            (forward_key.as_str(), "forward", 1.0_f32, state.can_forward()),
        ] {
            let node = self.new_named_node(
                key,
                Style {
                    width: Some(button_width),
                    height: Some(height),
                    ..Style::default()
                },
            );
            if enabled {
                self.register_interaction(node, key.to_string());
            }
            let hovered = enabled && self.pointer_in_previous(key);
            // The chevron's own box sits inside the padding.
            let cx = horizontal + arrow / 2.0;
            let cy = height / 2.0;
            let s = arrow * 0.22;
            let dx = s * direction;
            // MyGo dims a disabled element, and what is inside it, to half
            // its opacity (`ui/paint.go`), the arrow with the button.
            let color = if enabled {
                theme.foreground
            } else {
                mix_color(theme.muted, theme.background, 0.5)
            };
            let mut commands = vec![DrawCommand::SvgPath {
                    x: 0.0,
                    y: 0.0,
                    width: button_width,
                    height,
                    path: format!(
                        "M{} {}L{} {}L{} {}",
                        cx - dx / 2.0,
                        cy - s * 1.6,
                        cx + dx,
                        cy,
                        cx - dx / 2.0,
                        cy + s * 1.6
                    ),
                    fill: None,
                    stroke: Some(color),
                    stroke_width: 1.6,
                }];
            // A toolbar button shows its face only while the pointer is
            // over it, as MyGo's `styleFn` swaps in the hover color.
            if hovered {
                commands.insert(
                    0,
                    DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: button_width,
                        height,
                        radius: Radius::all(6.0),
                        color: theme.surface_hover,
                    },
                );
            }
            self.register_access_node(
                node,
                Role::Button,
                name,
                Some(if enabled { "enabled" } else { "disabled" }),
                false,
            );
            self.nodes
                .get_mut(&node)
                .expect("history button exists")
                .commands = commands;
        }
        self.pop_parent();
        if self.clicked_in_previous(&back_key) {
            back_clicked = state.back();
        }
        if self.clicked_in_previous(&forward_key) {
            forward_clicked = state.forward();
        }
        self.register_access_node(root, Role::Group, id, None, false);
        BackForwardState {
            back_clicked,
            forward_clicked,
            back_enabled: state.can_back(),
            forward_enabled: state.can_forward(),
            page: state.current().to_string(),
        }
    }

    /// Creates a single-line search field with a clear button.
    pub fn search_field(&mut self, id: &str, text: &mut String) -> SearchFieldState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        let input_id = format!("{id}:input");
        let clear_key = format!("{id}:clear");
        let gap = 6.0;
        // MyGo's search field overrides its field's Space(2.5) horizontal
        // padding with Space(2), inside a border that takes layout space
        // (`ui/combobox.go` coreSearchField).
        let padding_x = 9.0;
        let padding_y = 6.0;
        let icon_size = 15.0;
        let clear_size = 16.0;
        let height = measured_text_height(font_size) + padding_y * 2.0 + 2.0;
        let has_text = !text.is_empty();
        let input_width = (width
            - padding_x * 2.0
            - icon_size
            - gap
            - if has_text { clear_size + gap } else { 0.0 }
            - 2.0)
            .max(40.0);
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                flex_direction: FlexDirection::Row,
                gap,
                align_items: AlignItems::Center,
                padding: Edges::symmetric(padding_y, padding_x),
                ..Style::default()
            },
        );
        self.register_access_node(node, Role::Group, id, None, false);
        {
            let node_ref = self.nodes.get_mut(&node).expect("search node exists");
            node_ref.fill = Some((theme.surface, Radius::all(6.0)));
            node_ref.stroke = Some((theme.border, Radius::all(6.0)));
        }
        self.push_parent(node);
        let magnifier_id = format!("{id}:icon");
        self.icon(&magnifier_id, "magnifier", icon_size, theme.muted);
        let mut input = None;
        self.container().width(input_width).children(|context| {
            // MyGo's search field carries the placeholder its locale names
            // "Search" (`ui/combobox.go` coreSearchField).
            input = Some(context.text_field_impl(&input_id, text, None, false, Some("Search"), true));
        });
        let mut cleared = false;
        if has_text {
            let clear = self.new_named_node(
                &clear_key,
                Style {
                    width: Some(clear_size),
                    height: Some(clear_size),
                    ..Style::default()
                },
            );
            self.register_interaction(clear, clear_key.clone());
            let clicked = self.clicked_in_previous(&clear_key);
            // TextMuted at half over the field, with the X in the
            // background color, as MyGo clears a search.
            let clear_face = mix_color(theme.muted, theme.surface, 0.5);
            let mut commands = vec![DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: clear_size,
                height: clear_size,
                radius: Radius::all(8.0),
                color: clear_face,
            }];
            for (from, to) in [((4.8, 4.8), (11.2, 11.2)), ((11.2, 4.8), (4.8, 11.2))] {
                commands.push(DrawCommand::Line {
                    x1: from.0,
                    y1: from.1,
                    x2: to.0,
                    y2: to.1,
                    width: 1.5,
                    color: theme.background,
                });
            }
            self.register_access_node(clear, Role::Button, "clear search", None, false);
            self.nodes
                .get_mut(&clear)
                .expect("search clear exists")
                .commands = commands;
            if clicked {
                text.clear();
                self.focused_id = Some(input_id.clone());
                cleared = true;
            }
        }
        self.pop_parent();
        let field = input.expect("search input exists");
        if field.focused && self.key_pressed(Key::Escape) {
            text.clear();
            self.focused_id = Some(input_id.clone());
            cleared = true;
        }
        let submitted = field.focused && self.key_pressed(Key::Enter);
        SearchFieldState {
            changed: field.changed || cleared,
            submitted,
            cleared,
            focused: field.focused || cleared,
            hovered: field.hovered,
            text: text.clone(),
        }
    }

    /// Creates a scrollable fixed-size grid view.
    #[allow(clippy::too_many_arguments)]
    pub fn grid_view(
        &mut self,
        id: &str,
        state: &mut GridViewState,
        item_count: usize,
        item_size: Size,
        columns: usize,
        gap: f32,
        viewport_height: f32,
        render: impl Fn(&mut Context, usize),
    ) -> GridViewResponse {
        let theme = self.theme.clone();
        let columns = columns.max(1);
        let item_width = if item_size.width.is_finite() && item_size.width > 0.0 {
            item_size.width
        } else {
            48.0
        };
        let item_height = if item_size.height.is_finite() && item_size.height > 0.0 {
            item_size.height
        } else {
            48.0
        };
        let gap = if gap.is_finite() && gap > 0.0 {
            gap
        } else {
            0.0
        };
        let row_height = item_height + gap;
        let viewport_height = viewport_height.max(1.0);
        let rows = item_count.div_ceil(columns);
        let total_height = rows as f32 * row_height;
        let max_scroll = (total_height - viewport_height).max(0.0);
        let offset = match self.widget_states.get(id) {
            Some(WidgetState::ScrollView { offset, .. }) => *offset,
            _ => 0.0,
        }
        .clamp(0.0, max_scroll);
        let start_row = if row_height > 0.0 {
            (offset / row_height).floor() as usize
        } else {
            0
        }
        .min(rows);
        let end_row = if row_height > 0.0 {
            ((offset + viewport_height) / row_height).ceil() as usize
        } else {
            0
        }
        .min(rows)
        .max(start_row);
        let visible_start = (start_row * columns).min(item_count);
        let visible_end = (end_row * columns).min(item_count);
        let mut clicked = None;
        for index in visible_start..visible_end {
            let key = format!("{id}:item:{index}");
            if self.clicked_in_previous(&key) {
                clicked = Some(index);
                self.focused_id = Some(key);
                break;
            }
        }
        let previous_selected = state.selected;
        if let Some(index) = clicked {
            state.selected = Some(index);
        }
        let selected = state.selected;
        let scroll = self.scroll_view(id, viewport_height, |context| {
            if start_row > 0 {
                context.container().height(start_row as f32 * row_height);
            }
            for row in start_row..end_row {
                context
                    .row()
                    .gap(gap)
                    .height(row_height)
                    .children(|context| {
                        for column in 0..columns {
                            let index = row * columns + column;
                            if index >= item_count {
                                break;
                            }
                            let cell = context.new_named_node(
                                &format!("{id}:item:{index}"),
                                Style {
                                    width: Some(item_width),
                                    height: Some(item_height),
                                    ..Style::default()
                                },
                            );
                            context.register_interaction(cell, format!("{id}:item:{index}"));
                            context.register_focusable(cell, format!("{id}:item:{index}"));
                            let hovered =
                                context.pointer_in_previous(&format!("{id}:item:{index}"));
                            let is_selected = selected == Some(index);
                            let mut commands = Vec::new();
                            if is_selected || hovered {
                                commands.push(DrawCommand::RoundedRect {
                                    x: 0.0,
                                    y: 0.0,
                                    width: item_width,
                                    height: item_height,
                                    radius: Radius::all(6.0),
                                    color: if is_selected {
                                        mix_color(theme.surface, theme.accent, 0.2)
                                    } else {
                                        mix_color(theme.surface, theme.foreground, 0.06)
                                    },
                                });
                            }
                            context
                                .nodes
                                .get_mut(&cell)
                                .expect("grid cell exists")
                                .commands = commands;
                            context.push_parent(cell);
                            render(context, index);
                            context.pop_parent();
                            context.register_access_node(
                                cell,
                                Role::Button,
                                &format!("grid item {index}"),
                                Some(if is_selected {
                                    "selected"
                                } else {
                                    "unselected"
                                }),
                                false,
                            );
                        }
                    });
            }
            let remaining = rows.saturating_sub(end_row) as f32 * row_height;
            if remaining > 0.0 {
                context.container().height(remaining);
            }
        });
        let changed = state.selected != previous_selected;
        GridViewResponse {
            selected: state.selected,
            changed,
            clicked,
            visible_start,
            visible_end,
            scroll: scroll.scroll.clamp(0.0, max_scroll),
            max_scroll,
        }
    }

    /// Draws a minimal bar or line chart.
    pub fn chart(
        &mut self,
        id: &str,
        values: &[f32],
        kind: ChartKind,
        height: f32,
        min: Option<f32>,
        max: Option<f32>,
    ) -> ChartResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let height = if height.is_finite() && height > 0.0 {
            height
        } else {
            100.0
        };
        let bounds = self.previous_regions.get(id).map(|region| region.bounds);
        let data = values
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .collect::<Vec<_>>();
        let auto_min = data.iter().copied().fold(f32::INFINITY, f32::min);
        let auto_max = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let (mut min, mut max) = match (min, max) {
            (Some(min), Some(max)) if min.is_finite() && max.is_finite() => (min, max),
            (Some(min), None) if min.is_finite() => (min, auto_max),
            (None, Some(max)) if max.is_finite() => (auto_min, max),
            _ => (auto_min, auto_max),
        };
        if !min.is_finite() {
            min = 0.0;
        }
        if !max.is_finite() {
            max = if min < 1.0 { 1.0 } else { min + 1.0 };
        }
        if max < min {
            std::mem::swap(&mut min, &mut max);
        }
        if (max - min).abs() <= f32::EPSILON {
            min -= 0.5;
            max += 0.5;
        }
        let padding = 8.0;
        let plot = Rect::new(
            padding,
            padding,
            (width - padding * 2.0).max(1.0),
            (height - padding * 2.0).max(1.0),
        );
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node, id.to_string());
        let mut commands = vec![
            DrawCommand::Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                color: theme.surface,
            },
            DrawCommand::Line {
                x1: plot.x,
                y1: plot.bottom(),
                x2: plot.right(),
                y2: plot.bottom(),
                width: 1.0,
                color: theme.border,
            },
        ];
        let range = (max - min).max(f32::EPSILON);
        let fraction = |value: f32| ((value - min) / range).clamp(0.0, 1.0);
        match kind {
            ChartKind::Bar if !data.is_empty() => {
                let slot = plot.width / data.len() as f32;
                let bar_width = (slot * 0.7).max(1.0);
                for (index, value) in data.iter().enumerate() {
                    let fill = fraction(*value);
                    let bar_height = fill * plot.height;
                    if bar_height <= 0.0 {
                        continue;
                    }
                    commands.push(DrawCommand::RoundedRect {
                        x: plot.x + slot * index as f32 + (slot - bar_width) / 2.0,
                        y: plot.bottom() - bar_height,
                        width: bar_width,
                        height: bar_height,
                        radius: Radius::all(2.0),
                        color: mix_color(
                            theme.accent,
                            theme.foreground,
                            index as f32 / data.len() as f32 * 0.2,
                        ),
                    });
                }
            }
            ChartKind::Line if !data.is_empty() => {
                let points = data
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let x = if data.len() == 1 {
                            plot.x + plot.width / 2.0
                        } else {
                            plot.x + plot.width * index as f32 / (data.len() - 1) as f32
                        };
                        (x, plot.bottom() - fraction(*value) * plot.height)
                    })
                    .collect::<Vec<_>>();
                for pair in points.windows(2) {
                    commands.push(DrawCommand::Line {
                        x1: pair[0].0,
                        y1: pair[0].1,
                        x2: pair[1].0,
                        y2: pair[1].1,
                        width: 2.0,
                        color: theme.accent,
                    });
                }
                for (x, y) in &points {
                    commands.push(DrawCommand::Circle {
                        x: *x,
                        y: *y,
                        radius: 2.0,
                        color: theme.accent,
                    });
                }
            }
            _ => {}
        }
        self.nodes
            .get_mut(&node)
            .expect("chart node exists")
            .commands = commands;
        self.register_access_node(node, Role::Group, id, None, false);
        ChartResponse {
            plot,
            bounds,
            points: data.len(),
            min,
            max,
            kind,
        }
    }

    /// Renders an inspector tree as an indented debug list.
    pub fn inspector(&mut self, id: &str, tree: &InspectorTree) -> InspectorResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.xs,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let mut rendered = 0;
        if tree.roots.is_empty() {
            self.text("Inspector: empty")
                .font_size(theme.font_sizes.small)
                .color(theme.muted);
        } else {
            for item in &tree.roots {
                self.inspector_item(item, 0, &mut rendered);
            }
        }
        self.pop_parent();
        self.register_access_node(root, Role::Group, id, None, false);
        InspectorResponse {
            items: rendered,
            total_commands: tree.total.total(),
        }
    }

    fn inspector_item(&mut self, item: &InspectorItem, depth: usize, rendered: &mut usize) {
        let theme = self.theme.clone();
        *rendered += 1;
        let indent = "  ".repeat(depth);
        let width = self.current_content_width();
        self.text(&format!(
            "{indent}{} [{:?}] {:?} {} paint={}",
            item.id,
            item.role,
            item.bounds,
            item.paint.summary(),
            item.paint.total()
        ))
        .font_size(theme.font_sizes.small)
        .color(theme.foreground)
        .max_width(width);
        for child in &item.children {
            self.inspector_item(child, depth + 1, rendered);
        }
    }

    /// Creates a color swatch with preset palette and hex input.
    pub fn color_picker(
        &mut self,
        id: &str,
        color: &mut Color,
        state: &mut ColorPickerState,
    ) -> ColorPickerResponse {
        let theme = self.theme.clone();
        let block = self.new_named_node(
            id,
            Style {
                width: Some(28.0),
                height: Some(28.0),
                ..Style::default()
            },
        );
        self.register_interaction(block, id.to_string());
        self.register_focusable(block, id.to_string());
        let block_hovered = self.pointer_in_previous(id);
        let block_clicked = self.clicked_in_previous(id);
        let block_focused = self.focused_id.as_deref() == Some(id);
        if block_clicked || (block_focused && self.key_pressed(Key::Enter)) {
            state.open = !state.open;
            if state.open {
                let (hue, saturation, value) = rgb_to_hsv(*color);
                state.hue = hue;
                state.saturation = saturation;
                state.value = value;
                state.alpha = color.a;
                state.hex = color.to_hex();
            }
        }
        self.nodes
            .get_mut(&block)
            .expect("color block exists")
            .commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 28.0,
                height: 28.0,
                radius: Radius::all(6.0),
                color: *color,
            },
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 28.0,
                height: 28.0,
                radius: Radius::all(11.0),
                color: if block_hovered || block_focused {
                    theme.accent
                } else {
                    theme.border
                },
            },
        ];
        // Re-add the swatch on top of the border.
        self.nodes
            .get_mut(&block)
            .expect("color block exists")
            .commands
            .push(DrawCommand::RoundedRect {
                x: 2.0,
                y: 2.0,
                width: 24.0,
                height: 24.0,
                radius: Radius::all(5.0),
                color: *color,
            });
        self.register_access_node(
            block,
            Role::Button,
            "color",
            Some(color.to_hex().as_str()),
            block_focused,
        );
        let anchor = self.previous_regions.get(id).map(|region| {
            let bounds = region.bounds;
            Rect::new(bounds.x, bounds.y, 254.0, bounds.height)
        });
        let mut changed = false;
        let mut preset = None;
        let mut valid_hex = true;
        if state.open {
            if let Some(anchor) = anchor {
                let mut overlay_state = OverlayState::new(true);
                let mut hex = state.hex.clone();
                let mut dismiss = false;
                let popup_field_width = 238.0_f32;
                self.embedded_overlay = true;
                let response = self.overlay(
                    &format!("{id}:popup"),
                    &mut overlay_state,
                    anchor,
                    |context| {
                        context.column().width(popup_field_width).gap(0.0).children(|context| {
                            let (derived_hue, derived_sat, derived_val) = rgb_to_hsv(*color);
                            if derived_sat > 0 && derived_val > 0 {
                                state.hue = derived_hue;
                            }
                            state.saturation = derived_sat;
                            state.value = derived_val;
                            let field_width = popup_field_width;
                            let field_height = 198.0_f32;
                            let field_key = format!("{id}:field");
                            let field_node = context.new_named_node(
                                &field_key,
                                Style {
                                    width: Some(field_width),
                                    height: Some(field_height),
                                    ..Style::default()
                                },
                            );
                            context.register_interaction(field_node, field_key.clone());
                            if context.clicked_in_previous(&field_key) {
                                if let Some(point) = context.local_pointer_in_previous(&field_key) {
                                    state.saturation = ((point.x / field_width) * 100.0)
                                        .round()
                                        .clamp(0.0, 100.0)
                                        as u8;
                                    state.value = (100.0 - (point.y / field_height) * 100.0)
                                        .round()
                                        .clamp(0.0, 100.0)
                                        as u8;
                                    *color = hsv_to_color(
                                        state.hue as f32,
                                        state.saturation as f32 / 100.0,
                                        state.value as f32 / 100.0,
                                    );
                                    color.a = state.alpha;
                                    hex = color.to_hex();
                                    changed = true;
                                }
                            }
                            let columns = 32_usize;
                            let rows = 20_usize;
                            let mut field_commands = Vec::with_capacity(columns * rows + 2);
                            for row in 0..rows {
                                for column in 0..columns {
                                    let saturation = column as f32 / (columns - 1) as f32;
                                    let value = 1.0 - row as f32 / (rows - 1) as f32;
                                    field_commands.push(DrawCommand::Rect {
                                        x: column as f32 * field_width / columns as f32,
                                        y: row as f32 * field_height / rows as f32,
                                        width: field_width / columns as f32 + 0.5,
                                        height: field_height / rows as f32 + 0.5,
                                        color: hsv_to_color(state.hue as f32, saturation, value),
                                    });
                                }
                            }
                            let marker_x = state.saturation as f32 / 100.0 * field_width;
                            let marker_y = (1.0 - state.value as f32 / 100.0) * field_height;
                            // MyGo rings the chosen color with a white
                            // stroke two DIPs wide, its border filled
                            // inward from the snapped box, and a black
                            // 30% ring one wider around it
                            // (`ui/colorpicker.go`). A rounded rect of
                            // half-size radius is that ring's circle.
                            field_commands.push(DrawCommand::RoundedRect {
                                x: marker_x - 7.0,
                                y: marker_y - 7.0,
                                width: 14.0,
                                height: 14.0,
                                radius: Radius::all(7.0),
                                color: Color::rgb(255, 255, 255),
                            });
                            field_commands.push(DrawCommand::RoundedRect {
                                x: marker_x - 8.0,
                                y: marker_y - 8.0,
                                width: 16.0,
                                height: 16.0,
                                radius: Radius::all(8.0),
                                color: Color::rgba(0, 0, 0, 77),
                            });
                            // The ring's middle is the field's own color
                            // there, which is the color chosen.
                            field_commands.push(DrawCommand::RoundedRect {
                                x: marker_x - 5.0,
                                y: marker_y - 5.0,
                                width: 10.0,
                                height: 10.0,
                                radius: Radius::all(5.0),
                                color: *color,
                            });
                            context
                                .nodes
                                .get_mut(&field_node)
                                .expect("color field exists")
                                .commands = field_commands;

                            let hue_width = popup_field_width;
                            let hue_height = 12.0_f32;
                            let hue_key = format!("{id}:hue");
                            let hue_node = context.new_named_node(
                                &hue_key,
                                Style {
                                    width: Some(hue_width),
                                    height: Some(hue_height),
                                    ..Style::default()
                                },
                            );
                            context.register_interaction(hue_node, hue_key.clone());
                            if context.clicked_in_previous(&hue_key) {
                                if let Some(point) = context.local_pointer_in_previous(&hue_key) {
                                    state.hue = ((point.x / hue_width) * 360.0)
                                        .round()
                                        .clamp(0.0, 359.0)
                                        as u16;
                                    if state.saturation == 0 || state.value == 0 {
                                        state.saturation = 100;
                                        state.value = 100;
                                    }
                                    *color = hsv_to_color(
                                        state.hue as f32,
                                        state.saturation as f32 / 100.0,
                                        state.value as f32 / 100.0,
                                    );
                                    color.a = state.alpha;
                                    hex = color.to_hex();
                                    changed = true;
                                }
                            }
                            let hue_steps = 24_usize;
                            let mut hue_commands = Vec::with_capacity(hue_steps + 2);
                            for step in 0..hue_steps {
                                hue_commands.push(DrawCommand::Rect {
                                    x: step as f32 * hue_width / hue_steps as f32,
                                    y: 0.0,
                                    width: hue_width / hue_steps as f32 + 0.5,
                                    height: hue_height,
                                    color: hsv_to_color(
                                        (step as f32 + 0.5) / hue_steps as f32 * 360.0,
                                        1.0,
                                        1.0,
                                    ),
                                });
                            }
                            let hue_marker = (state.hue as f32 / 360.0 * hue_width)
                                .clamp(6.0, hue_width - 6.0);
                            hue_commands.push(DrawCommand::Circle {
                                x: hue_marker,
                                y: hue_height / 2.0,
                                radius: 6.0,
                                color: Color::rgb(255, 255, 255),
                            });
                            hue_commands.push(DrawCommand::Circle {
                                x: hue_marker,
                                y: hue_height / 2.0,
                                radius: 4.0,
                                color: *color,
                            });
                            context
                                .nodes
                                .get_mut(&hue_node)
                                .expect("hue bar exists")
                                .commands = hue_commands;

                            let alpha_width = popup_field_width;
                            let alpha_height = 12.0_f32;
                            let alpha_key = format!("{id}:alpha");
                            let alpha_node = context.new_named_node(
                                &alpha_key,
                                Style {
                                    width: Some(alpha_width),
                                    height: Some(alpha_height),
                                    ..Style::default()
                                },
                            );
                            context.register_interaction(alpha_node, alpha_key.clone());
                            if context.clicked_in_previous(&alpha_key) {
                                if let Some(point) = context.local_pointer_in_previous(&alpha_key) {
                                    state.alpha = ((point.x / alpha_width) * 255.0)
                                        .round()
                                        .clamp(0.0, 255.0)
                                        as u8;
                                    color.a = state.alpha;
                                    hex = color.to_hex();
                                    changed = true;
                                }
                            }
                            let check = 4.0_f32;
                            let check_columns = (alpha_width / check).ceil() as usize;
                            let check_rows = (alpha_height / check).ceil() as usize;
                            let mut alpha_commands = Vec::with_capacity(check_columns * check_rows + 3);
                            for row in 0..check_rows {
                                for column in 0..check_columns {
                                    alpha_commands.push(DrawCommand::Rect {
                                        x: column as f32 * check,
                                        y: row as f32 * check,
                                        width: check + 0.5,
                                        height: check + 0.5,
                                        color: if (row + column) % 2 == 0 {
                                            Color::rgb(226, 232, 240)
                                        } else {
                                            Color::rgb(148, 163, 184)
                                        },
                                    });
                                }
                            }
                            let alpha_steps = 24_usize;
                            for step in 0..alpha_steps {
                                alpha_commands.push(DrawCommand::Rect {
                                    x: step as f32 * alpha_width / alpha_steps as f32,
                                    y: 0.0,
                                    width: alpha_width / alpha_steps as f32 + 0.5,
                                    height: alpha_height,
                                    color: Color::rgba(
                                        color.r,
                                        color.g,
                                        color.b,
                                        (((step as f32 + 0.5) / alpha_steps as f32) * 255.0)
                                            .round()
                                            .clamp(0.0, 255.0)
                                            as u8,
                                    ),
                                });
                            }
                            let alpha_marker = (state.alpha as f32 / 255.0 * alpha_width)
                                .clamp(6.0, alpha_width - 6.0);
                            alpha_commands.push(DrawCommand::Circle {
                                x: alpha_marker,
                                y: alpha_height / 2.0,
                                radius: 6.0,
                                color: Color::rgb(255, 255, 255),
                            });
                            alpha_commands.push(DrawCommand::Circle {
                                x: alpha_marker,
                                y: alpha_height / 2.0,
                                radius: 4.0,
                                color: *color,
                            });
                            context
                                .nodes
                                .get_mut(&alpha_node)
                                .expect("alpha bar exists")
                                .commands = alpha_commands;

                            let hex_id = format!("{id}:hex");
                            let field = context.text_field(&hex_id, &mut hex);
                            if field.focused && context.key_pressed(Key::Enter) {
                                match Color::from_hex(&hex) {
                                    Ok(next) => {
                                        *color = next;
                                        state.alpha = next.a;
                                        changed = true;
                                        valid_hex = true;
                                    }
                                    Err(_) => {
                                        hex = color.to_hex();
                                        valid_hex = false;
                                    }
                                }
                            }
                            for row_index in 0..2 {
                                context.row().gap(theme.spacing.xs).children(|context| {
                                    for column in 0..6 {
                                        let index = row_index * 6 + column;
                                        let key = format!("{id}:preset:{index}");
                                        let cell = context.new_named_node(
                                            &key,
                                            Style {
                                                width: Some(24.0),
                                                height: Some(24.0),
                                                ..Style::default()
                                            },
                                        );
                                        context.register_interaction(cell, key.clone());
                                        let cell_hovered = context.pointer_in_previous(&key);
                                        context
                                            .nodes
                                            .get_mut(&cell)
                                            .expect("preset exists")
                                            .commands = vec![
                                            DrawCommand::RoundedRect {
                                                x: 0.0,
                                                y: 0.0,
                                                width: 24.0,
                                                height: 24.0,
                                                radius: Radius::all(4.0),
                                                color: PRESET_COLORS[index],
                                            },
                                            DrawCommand::RoundedRect {
                                                x: 0.0,
                                                y: 0.0,
                                                width: 24.0,
                                                height: 24.0,
                                                radius: Radius::all(if cell_hovered {
                                                    10.0
                                                } else {
                                                    4.0
                                                }),
                                                color: if *color == PRESET_COLORS[index]
                                                    || cell_hovered
                                                {
                                                    theme.accent
                                                } else {
                                                    theme.border
                                                },
                                            },
                                            DrawCommand::RoundedRect {
                                                x: 2.0,
                                                y: 2.0,
                                                width: 20.0,
                                                height: 20.0,
                                                radius: Radius::all(3.0),
                                                color: PRESET_COLORS[index],
                                            },
                                        ];
                                        context.register_access_node(
                                            cell,
                                            Role::Button,
                                            &format!("preset {index}"),
                                            None,
                                            false,
                                        );
                                        if context.clicked_in_previous(&key) {
                                            *color = PRESET_COLORS[index];
                                            state.alpha = color.a;
                                            hex = color.to_hex();
                                            preset = Some(index);
                                            changed = true;
                                            dismiss = true;
                                        }
                                    }
                                });
                            }
                        });
                    },
                );
                state.hex = hex;
                state.open = overlay_state.open && response.open && !dismiss;
                if dismiss {
                    self.close_overlay_runtime(&format!("{id}:popup"));
                }
                if !state.open {
                    self.focused_id = Some(id.to_string());
                }
            }
        }
        ColorPickerResponse {
            color: *color,
            changed,
            open: state.open,
            preset,
            valid_hex,
        }
    }

    /// Draws MyGo's `ColorPicker`: a panel always open, 240 DIPs wide, of a
    /// saturation/brightness square, hue and opacity sliders, the color in
    /// hex, and swatches of common colors (`ui/colorpicker.go`).
    pub fn color_panel(
        &mut self,
        id: &str,
        color: &mut Color,
        state: &mut ColorPanelState,
    ) -> ColorPanelResponse {
        let theme = self.theme.clone();
        let space = theme.spacing.base;
        let radius = 6.0_f32;
        // The app set another color: take it, as MyGo's picker does.
        if *color != state.last {
            state.hsv = Hsv::from_color(*color);
            state.last = *color;
            state.hex = hex_of(*color);
        }
        let mut changed = false;
        let width = space * 60.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: space * 2.5,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let hsv = state.hsv;

        // The square of saturation and brightness.
        let square_id = format!("{id}:square");
        let square_height = space * 40.0;
        let square = self.new_named_node(
            &square_id,
            Style {
                width: Some(width),
                height: Some(square_height),
                ..Style::default()
            },
        );
        self.register_interaction(square, square_id.clone());
        self.register_focusable(square, square_id.clone());
        let square_hovered = self.pointer_in_previous(&square_id);
        if square_hovered && self.input.pointer_down {
            state.dragging = Some(PanelDrag::Square);
            self.focused_id = Some(square_id.clone());
        }
        if state.dragging == Some(PanelDrag::Square) {
            if let Some(point) = self.local_pointer_in_previous(&square_id) {
                if let Some(bounds) = self.previous_regions.get(&square_id).map(|r| r.bounds) {
                    let left = bounds.x;
                    let top = bounds.y;
                    let w = bounds.width.max(1.0);
                    let h = bounds.height.max(1.0);
                    let saturation = ((point.x - left) / w).clamp(0.0, 1.0);
                    let value = 1.0 - ((point.y - top) / h).clamp(0.0, 1.0);
                    changed |= set_hsv(state, color, Hsv { saturation, value, ..hsv });
                }
            }
            if !self.input.pointer_down {
                state.dragging = None;
            }
        }
        let square_focused = self.focused_id.as_deref() == Some(square_id.as_str());
        let hsv = state.hsv;
        if square_focused {
            let next = if self.key_pressed(Key::ArrowRight) {
                Some(Hsv { saturation: hsv.saturation + 0.01, ..hsv })
            } else if self.key_pressed(Key::ArrowLeft) {
                Some(Hsv { saturation: hsv.saturation - 0.01, ..hsv })
            } else if self.key_pressed(Key::ArrowUp) {
                Some(Hsv { value: hsv.value + 0.01, ..hsv })
            } else if self.key_pressed(Key::ArrowDown) {
                Some(Hsv { value: hsv.value - 0.01, ..hsv })
            } else {
                None
            };
            if let Some(next) = next {
                changed |= set_hsv(state, color, next);
            }
        }
        let hsv = state.hsv;
        let hue_color = Hsv { saturation: 1.0, value: 1.0, alpha: 1.0, ..hsv }.to_color();
        let mut commands = vec![
            DrawCommand::LinearGradient {
                x: 0.0,
                y: 0.0,
                width,
                height: square_height,
                radius: Radius::all(radius),
                start: Point::new(0.0, 0.5),
                end: Point::new(1.0, 0.5),
                stops: vec![
                    GradientStop { offset: 0.0, color: Color::rgb(255, 255, 255) },
                    GradientStop { offset: 1.0, color: hue_color },
                ],
            },
            DrawCommand::LinearGradient {
                x: 0.0,
                y: 0.0,
                width,
                height: square_height,
                radius: Radius::all(radius),
                start: Point::new(0.5, 0.0),
                end: Point::new(0.5, 1.0),
                stops: vec![
                    GradientStop { offset: 0.0, color: Color::rgba(0, 0, 0, 0) },
                    GradientStop { offset: 1.0, color: Color::rgb(0, 0, 0) },
                ],
            },
        ];
        // The color chosen, ringed: a white ring inside a dark one
        // (`ui/colorpicker.go` strokes the square's mark twice). MyGo's
        // Painter fills a stroke's border inward from the snapped box
        // (`ui/paint.go` borders), so the rings are filled discs one DIP
        // apart, the middle the color the square shows there.
        let marker_x = width * hsv.saturation;
        let marker_y = square_height * (1.0 - hsv.value);
        let d = space * 3.5;
        commands.push(DrawCommand::RoundedRect {
            x: marker_x - d / 2.0 - 1.0,
            y: marker_y - d / 2.0 - 1.0,
            width: d + 2.0,
            height: d + 2.0,
            radius: Radius::all(d / 2.0 + 1.0),
            color: Color::rgba(0, 0, 0, 77),
        });
        commands.push(DrawCommand::RoundedRect {
            x: marker_x - d / 2.0,
            y: marker_y - d / 2.0,
            width: d,
            height: d,
            radius: Radius::all(d / 2.0),
            color: Color::rgb(255, 255, 255),
        });
        let inner = d - 4.0;
        commands.push(DrawCommand::RoundedRect {
            x: marker_x - inner / 2.0,
            y: marker_y - inner / 2.0,
            width: inner,
            height: inner,
            radius: Radius::all(inner / 2.0),
            color: hsv_to_color(hsv.hue, hsv.saturation, hsv.value),
        });
        if self.shows_focus_ring(square_focused) {
            commands.extend(focus_ring(0.0, 0.0, width, square_height, theme.accent));
        }
        self.set_node_commands(square, commands);
        self.register_access_node(square, Role::Slider, "Saturation", None, square_focused);

        // The hue, and the opacity.
        let hues = [
            Color::rgb(255, 0, 0),
            Color::rgb(255, 255, 0),
            Color::rgb(0, 255, 0),
            Color::rgb(0, 255, 255),
            Color::rgb(0, 0, 255),
            Color::rgb(255, 0, 255),
            Color::rgb(255, 0, 0),
        ];
        let mut hue = hsv.hue;
        let hue_changed = self.channel_slider(
            &format!("{id}:hue"),
            &mut hue,
            360.0,
            "Hue",
            move |width, bar_y, bar_height| {
                // Six bands, each interpolating one hue step to the next, as
                // MyGo paints the hue track (`ui/colorpicker.go`
                // channelSlider): a single seven-stop gradient would take a
                // different path through the same colors. MyGo clips the
                // track to a capsule, so only the two outer ends look round;
                // rounding the outer corners here gives the same outline
                // without the notches a chain of pills would leave.
                let band = width / 6.0;
                let mut commands = Vec::with_capacity(6);
                for index in 0..6 {
                    let radius = match index {
                        0 => Radius::new(bar_height / 2.0, 0.0, 0.0, bar_height / 2.0),
                        5 => Radius::new(0.0, bar_height / 2.0, bar_height / 2.0, 0.0),
                        _ => Radius::all(0.0),
                    };
                    // MyGo overlaps each band by half a pixel, but clips the
                    // last one to the track: its round end is the capsule's,
                    // so the last band stops at the track's width.
                    let band_width = if index == 5 { band } else { band + 0.5 };
                    commands.push(DrawCommand::LinearGradient {
                        x: band * index as f32,
                        y: bar_y,
                        width: band_width,
                        height: bar_height,
                        radius,
                        start: Point::new(0.0, 0.5),
                        end: Point::new(1.0, 0.5),
                        stops: vec![
                            GradientStop { offset: 0.0, color: hues[index] },
                            GradientStop { offset: 1.0, color: hues[index + 1] },
                        ],
                    });
                }
                commands
            },
        );
        if hue_changed {
            changed |= set_hsv(state, color, Hsv { hue, ..hsv });
        }
        let hsv = state.hsv;
        let opaque = Hsv { alpha: 1.0, ..hsv }.to_color();
        let mut alpha = hsv.alpha;
        let alpha_changed = self.channel_slider(
            &format!("{id}:alpha"),
            &mut alpha,
            1.0,
            "Opacity",
            move |width, bar_y, bar_height| {
                // The checkerboard shows through the gradient, as MyGo
                // paints `checkers` before the color (`ui/colorpicker.go`
                // channelSlider).
                let mut commands =
                    checkerboard_commands(width, bar_y, bar_height, space * 1.5, bar_height / 2.0);
                commands.push(DrawCommand::LinearGradient {
                    x: 0.0,
                    y: bar_y,
                    width,
                    height: bar_height,
                    radius: Radius::all(bar_height / 2.0),
                    start: Point::new(0.0, 0.5),
                    end: Point::new(1.0, 0.5),
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: Color::rgba(opaque.r, opaque.g, opaque.b, 0),
                        },
                        GradientStop { offset: 1.0, color: opaque },
                    ],
                });
                commands
            },
        );
        if alpha_changed {
            changed |= set_hsv(state, color, Hsv { alpha, ..hsv });
        }

        // The color, and its hex. The field grows into what the swatch and
        // the gap leave of the panel's width.
        let swatch_size = space * 7.0;
        let now = *color;
        let field_width = (width - swatch_size - space * 2.0).max(1.0);
        self.row().gap(space * 2.0).align(AlignItems::Center).children(|context| {
            let box_id = format!("{id}:well");
            let swatch = context.new_named_node(
                &box_id,
                Style {
                    width: Some(swatch_size),
                    height: Some(swatch_size),
                    ..Style::default()
                },
            );
            context.set_node_commands(
                swatch,
                vec![
                    DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: swatch_size,
                        height: swatch_size,
                        radius: Radius::all(radius),
                        color: theme.surface,
                    },
                    DrawCommand::RoundedRect {
                        x: 1.0,
                        y: 1.0,
                        width: swatch_size - 2.0,
                        height: swatch_size - 2.0,
                        radius: Radius::all(radius - 1.0),
                        color: now,
                    },
                ],
            );
            let hex_id = format!("{id}:hex");
            let mut text = state.hex.clone();
            let mut field = TextFieldState {
                changed: false,
                cursor: 0,
                focused: false,
                hovered: false,
            };
            context.container().width(field_width).children(|context| {
                field = context.text_field_impl(&hex_id, &mut text, None, false, None, false);
            });
            state.hex = text;
            if field.changed {
                if let Ok(next) = Color::from_hex(&state.hex) {
                    changed |= next != *color;
                    *color = next;
                    state.hsv = Hsv::from_color(next);
                    state.last = next;
                }
            }
            if !field.focused && state.hex != hex_of(*color) {
                state.hex = hex_of(*color);
            }
        });

        // Swatches.
        self.grid().columns(6).gap(space * 1.5).children(|context| {
            for (index, swatch) in SWATCH_COLORS.iter().enumerate() {
                let key = format!("{id}:swatch:{index}");
                // A swatch stretches across its column, as MyGo's Fr(1)
                // tracks do, at Space(5) high.
                let cell_width = (width - space * 1.5 * 5.0) / 6.0;
                let cell_height = space * 5.0;
                let cell = context.new_named_node(
                    &key,
                    Style {
                        width: Some(cell_width),
                        height: Some(cell_height),
                        ..Style::default()
                    },
                );
                context.register_interaction(cell, key.clone());
                let selected = *color == *swatch;
                // MyGo's Border(2) fills two pixels inside the box, so a
                // chosen swatch shows the theme's text color as a ring
                // around the color itself.
                let mut commands = Vec::with_capacity(2);
                if selected {
                    commands.push(DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: cell_width,
                        height: cell_height,
                        radius: Radius::all(radius),
                        color: theme.foreground,
                    });
                }
                let inset = if selected { 2.0 } else { 0.0 };
                commands.push(DrawCommand::RoundedRect {
                    x: inset,
                    y: inset,
                    width: cell_width - inset * 2.0,
                    height: cell_height - inset * 2.0,
                    radius: Radius::all((radius - inset).max(0.0)),
                    color: *swatch,
                });
                context.set_node_commands(cell, commands);
                context.register_access_node(
                    cell,
                    Role::Button,
                    SWATCH_NAMES[index],
                    None,
                    false,
                );
                if context.clicked_in_previous(&key) {
                    changed |= set_hsv(state, color, Hsv::from_color(*swatch));
                }
            }
        });
        self.pop_parent();
        ColorPanelResponse {
            color: *color,
            changed,
        }
    }

    /// Draws one channel of a color panel: a rounded track the caller
    /// paints, and a white knob half the track's padding inside its ends,
    /// as MyGo's `channelSlider` (`ui/colorpicker.go`).
    fn channel_slider(
        &mut self,
        id: &str,
        value: &mut f32,
        max: f32,
        label: &str,
        track: impl FnOnce(f32, f32, f32) -> Vec<DrawCommand>,
    ) -> bool {
        let theme = self.theme.clone();
        let space = theme.spacing.base;
        let knob = space * 3.0;
        let height = space * 4.0;
        let width = self.current_content_width();
        *value = value.clamp(0.0, max);
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node, id.to_string());
        self.register_focusable(node, id.to_string());
        let hovered = self.pointer_in_previous(id);
        if hovered && self.input.pointer_down {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let dragging = self.widget_states.get(id).is_some_and(|state| {
            matches!(state, WidgetState::Slider { dragging } if *dragging)
        });
        let mut changed = false;
        let pressed = hovered && self.input.pointer_down;
        if pressed || dragging {
            if let Some(point) = self.local_pointer_in_previous(id) {
                if let Some(bounds) = self.previous_regions.get(id).map(|r| r.bounds) {
                    let inner = (bounds.width - knob).max(1.0);
                    let fraction =
                        ((point.x - bounds.x - knob / 2.0) / inner).clamp(0.0, 1.0);
                    let next = fraction * max;
                    if (next - *value).abs() > f32::EPSILON {
                        *value = next;
                        changed = true;
                    }
                }
            }
        }
        self.widget_states.insert(
            id.to_string(),
            WidgetState::Slider {
                dragging: pressed || dragging,
            },
        );
        let fraction = if max > 0.0 { (*value / max).clamp(0.0, 1.0) } else { 0.0 };
        // The track is the node less Space(0.5) above and below, clipped to
        // a capsule as MyGo's `Clip(bar, bar.H/2)` does.
        let bar_y = space * 0.5;
        let bar_height = height - space;
        let mut commands = vec![
            DrawCommand::ScissorPush {
                rect: Rect::new(0.0, bar_y, width, bar_height),
            },
            DrawCommand::RoundedRect {
                x: 0.0,
                y: bar_y,
                width,
                height: bar_height,
                radius: Radius::all(bar_height / 2.0),
                color: Color::rgb(255, 255, 255),
            },
        ];
        commands.extend(track(width, bar_y, bar_height));
        commands.push(DrawCommand::ScissorPop);
        let knob_x = knob / 2.0 + (width - knob) * fraction;
        let mut knob_commands = vec![
            DrawCommand::Shadow {
                x: knob_x - knob / 2.0,
                y: 0.0,
                width: knob,
                height,
                radius: knob / 2.0,
                offset_x: 0.0,
                offset_y: 1.0,
                blur: 3.0,
                color: Color::rgba(0, 0, 0, 77),
            },
            DrawCommand::RoundedRect {
                x: knob_x - knob / 2.0,
                y: 0.0,
                width: knob,
                height,
                radius: Radius::all(knob / 2.0),
                color: Color::rgb(255, 255, 255),
            },
        ];
        if self.shows_focus_ring(focused) {
            knob_commands.extend(focus_ring(
                knob_x - knob / 2.0,
                0.0,
                knob,
                height,
                theme.accent,
            ));
        }
        commands.extend(knob_commands);
        self.set_node_commands(node, commands);
        self.register_access_node(node, Role::Slider, label, None, focused);
        changed
    }

    /// Creates a HH:MM time input with step buttons.
    pub fn time_input(&mut self, id: &str, text: &mut String) -> TimeInputState {
        let theme = self.theme.clone();
        let space = theme.spacing.base;
        // MyGo's time field is one pill of the hours, the minutes and the
        // half of the day, each a segment the arrows step
        // (`ui/timeinput.go` coreTimeInput). The preview's locale writes
        // them as `07:30`, 24 hours, as this demo's page does.
        let (mut hour, mut minute) = parse_time(text).unwrap_or((7, 30));
        let pill = self.new_named_node(
            id,
            Style {
                // The line's height plus the padding and the border, as
                // MyGo lays the field out.
                height: Some((theme.font_sizes.body * 1.5).round() + space * 3.0 - 2.0),
                flex_direction: FlexDirection::Row,
                gap: 0.0,
                // MyGo's field is `Padding(Space(1.5), Space(2))` inside a
                // 1 DIP border, which its layout gives a pixel of room.
                padding: Edges {
                    top: space * 1.5 + 1.0,
                    right: space * 2.0 + 1.0,
                    bottom: space * 1.5 + 1.0,
                    left: space * 2.0 + 1.0,
                },
                align_items: AlignItems::Center,
                ..Style::default()
            },
        );
        self.nodes
            .get_mut(&pill)
            .expect("time pill exists")
            .fill = Some((theme.surface, Radius::all(6.0)));
        self.nodes
            .get_mut(&pill)
            .expect("time pill exists")
            .stroke = Some((theme.border, Radius::all(6.0)));
        self.push_parent(pill);
        let mut changed = false;
        let mut focused = false;
        let mut hovered = false;
        // The hours and the minutes, each stepped by the arrows while it
        // has the focus, with the separator between them.
        for (index, (key, digits, top)) in [("hours", 2usize, 24i32), ("minutes", 2, 60)]
            .into_iter()
            .enumerate()
        {
            if index == 1 {
                // MyGo's separator takes `Padding(0, 1)` of its own.
                self.container().padding_x(1.0).children(|context| {
                    context
                        .text(":")
                        .font_size(theme.font_sizes.body)
                        .color(theme.muted);
                });
            }
            let value = if index == 0 { hour } else { minute };
            let label = format!("{value:0digits$}");
            let glyphs =
                host_text_width_weighted(&label, theme.font_sizes.body, false, false, 400)
                    .unwrap_or(theme.font_sizes.body * 0.6 * digits as f32);
            let segment_id = format!("{id}:{key}");
            let segment = self.new_named_node(
                &segment_id,
                Style {
                    width: Some(glyphs + space),
                    height: Some(theme.font_sizes.body * 1.5),
                    padding: Edges {
                        top: 0.0,
                        right: space * 0.5,
                        bottom: 0.0,
                        left: space * 0.5,
                    },
                    ..Style::default()
                },
            );
            self.register_interaction(segment, segment_id.clone());
            self.register_focusable(segment, segment_id.clone());
            // A click takes the segment's focus, as MyGo's segments do
            // (`ui/timeinput.go` coreTimeInput's Focusable boxes).
            if self.clicked_in_previous(&segment_id) {
                self.focused_id = Some(segment_id.clone());
            }
            let is_focused = self.focused_id.as_deref() == Some(segment_id.as_str());
            if is_focused {
                focused = true;
                self.nodes.get_mut(&segment).expect("segment").fill =
                    Some((theme.accent, Radius::all(space)));
            }
            if self.pointer_in_previous(&segment_id) {
                hovered = true;
            }
            if is_focused {
                let step = if self.key_pressed(Key::ArrowUp) {
                    1
                } else if self.key_pressed(Key::ArrowDown) {
                    -1
                } else {
                    0
                };
                if step != 0 {
                    let next = (value as i32 + step).rem_euclid(top);
                    if index == 0 {
                        hour = next as u32;
                    } else {
                        minute = next as u32;
                    }
                    *text = format_time(hour, minute);
                    changed = true;
                }
            }
            self.nodes.get_mut(&segment).expect("segment").commands = vec![DrawCommand::Text {
                // The segment's text sits inside its `Padding(0, Space(0.5))`,
                // as every text node draws inside its own padding.
                x: space * 0.5,
                y: 0.0,
                text: label,
                font_size: theme.font_sizes.body,
                weight: 400,
                italic: false,
                mono: false,
                family: None,
                color: if is_focused {
                    theme.background
                } else {
                    theme.foreground
                },
                max_width: Some(glyphs.max(1.0)),
                alignment: TextAlignment::Center,
            }];
            self.register_access_node(segment, Role::Slider, &segment_id, None, false);
        }
        self.pop_parent();
        self.register_access_node(pill, Role::Group, id, None, false);
        TimeInputState {
            changed,
            // The field reports what its text parses to, as MyGo's
            // `coreTimeInput` keeps the time it was given.
            valid: parse_time(text).is_some(),
            hour,
            minute,
            submitted: changed,
            focused,
            hovered,
        }
    }

    /// Creates a toolbar with a tail overflow menu.
    pub fn toolbar(
        &mut self,
        id: &str,
        state: &mut ToolbarState,
        items: &[ToolbarItem<'_>],
        available_width: f32,
        mut render: impl FnMut(&mut Context, usize),
    ) -> ToolbarResponse {
        let theme = self.theme.clone();
        let width = if available_width.is_finite() && available_width > 0.0 {
            available_width
        } else {
            self.current_content_width()
        };
        let gap = theme.spacing.xs;
        let visible = overflow_split(items, width, gap);
        let hidden = items.len().saturating_sub(visible);
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Row,
                gap,
                ..Style::default()
            },
        );
        self.push_parent(root);
        for (index, item) in items.iter().take(visible).enumerate() {
            self.container().width(item.width).children(|context| {
                render(context, index);
            });
        }
        let mut selected = None;
        let mut overflow_open = false;
        if hidden > 0 {
            let overflow_id = format!("{id}:overflow");
            let labels = items[visible..]
                .iter()
                .map(|item| item.label)
                .collect::<Vec<_>>();
            let response = self.menu_button(&overflow_id, "…", &mut state.overflow, &labels);
            overflow_open = response.open;
            if let Some(index) = response.selected {
                selected = Some(visible + index);
            }
        }
        self.pop_parent();
        self.register_access_node(root, Role::Group, id, None, false);
        ToolbarResponse {
            visible,
            overflow: hidden,
            selected,
            overflow_open,
        }
    }

    /// Creates a pressed-state toggle button.
    pub fn toggle(
        &mut self,
        id: &str,
        label: &str,
        active: &mut bool,
        mut group: Option<&mut ToggleGroup>,
    ) -> ToggleState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = measure_text_width(label, font_size) + theme.spacing.md * 2.0;
        let height = 30.0;
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node, id.to_string());
        self.register_focusable(node, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let clicked = self.clicked_in_previous(id) || (focused && self.key_pressed(Key::Space));
        let previous_group = group.as_ref().and_then(|group| group.active.clone());
        if let Some(group) = group.as_deref_mut() {
            if clicked {
                group.active = if group.active.as_deref() == Some(id) {
                    None
                } else {
                    Some(id.to_string())
                };
            }
            *active = group.active.as_deref() == Some(id);
        } else if clicked {
            *active = !*active;
        }
        let active_now = *active;
        let group_after = group.as_ref().and_then(|group| group.active.clone());
        let changed = clicked
            && if group.is_some() {
                previous_group != group_after
            } else {
                true
            };
        let background = if active_now {
            theme.accent
        } else if hovered || focused {
            mix_color(theme.surface, theme.foreground, 0.08)
        } else {
            theme.surface
        };
        let text_color = if active_now {
            theme.on_accent
        } else {
            theme.foreground
        };
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(5.0),
                color: background,
            },
            DrawCommand::Text {
                x: theme.spacing.md,
                y: (height - measured_text_height(font_size)) / 2.0,
                text: label.to_string(),
                font_size,
                weight: if active_now { 600 } else { 400 },
                italic: false,
                mono: false,
                family: None,
                color: text_color,
                max_width: Some((width - theme.spacing.md * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            },
        ];
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.register_access_node(
            node,
            Role::Button,
            label,
            Some(if active_now { "active" } else { "inactive" }),
            focused,
        );
        self.nodes
            .get_mut(&node)
            .expect("toggle node exists")
            .commands = commands;
        ToggleState {
            active: active_now,
            changed,
            clicked,
            focused,
            hovered,
            group_active: group_after,
        }
    }

    /// Creates a MyGo segment: a centered icon or label in a box the
    /// caller styles, reporting clicks without owning the choice.
    ///
    /// MyGo draws toggles and segments as buttons whose face depends on
    /// the group they sit in (`ui/widgets.go` styleButton): a plain toggle
    /// has a Surface face with a 1px border, while a segment is transparent
    /// until hovered and paints the theme background when chosen.
    #[allow(clippy::too_many_arguments)]
    pub fn segment(
        &mut self,
        id: &str,
        label: &str,
        icon: Option<&str>,
        selected: bool,
        padding: Edges,
        radius: f32,
        face: SegmentFace,
        disabled: bool,
    ) -> SegmentState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let line = measured_text_height(font_size);
        let content_size = match icon {
            // MyGo sizes an icon at one em of the text around it, its
            // `intrinsicSize`, and the box is that em tall too; a toolbar's
            // history arrow is a `Space(4)` = 16 box instead (`historyButton`
            // in `ui/router.go`).
            Some(_) => {
                let side = if matches!(face, SegmentFace::Toolbar) {
                    theme.spacing.base * 4.0
                } else {
                    14.0
                };
                Size::new(side, side)
            }
            // Segment labels measure with the canvas at weight 400, the
            // face they draw with; the CPU estimate runs wide.
            None => Size::new(
                host_text_width_weighted(label, font_size, false, false, 400)
                    .unwrap_or_else(|| measure_text_width(label, font_size)),
                line,
            ),
        };
        let width = padding.left + content_size.width + padding.right;
        let height = padding.top + content_size.height + padding.bottom;
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node, id.to_string());
        self.register_focusable(node, id.to_string());
        let hovered = !disabled && self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = !disabled && self.focused_id.as_deref() == Some(id);
        let clicked = !disabled
            && (self.clicked_in_previous(id) || (focused && self.key_pressed(Key::Space)));
        let (fill, stroke) = match face {
            SegmentFace::Plain if !disabled => {
                let color = if selected || pressed {
                    theme.surface_pressed
                } else if hovered {
                    theme.surface_hover
                } else {
                    theme.surface
                };
                (Some(color), Some(theme.border))
            }
            SegmentFace::Plain => (None, None),
            SegmentFace::Segment if !disabled => {
                let color = if selected {
                    theme.background
                } else if pressed {
                    theme.surface_pressed
                } else if hovered {
                    theme.surface_hover
                } else {
                    Color::TRANSPARENT
                };
                (Some(color), None)
            }
            SegmentFace::Segment => (None, None),
            SegmentFace::Toolbar if !disabled => {
                let color = if selected || pressed {
                    theme.surface_pressed
                } else if hovered {
                    theme.surface_hover
                } else {
                    Color::TRANSPARENT
                };
                (Some(color), None)
            }
            SegmentFace::Toolbar => (None, None),
        };
        let ring_visible = self.shows_focus_ring(focused);
        // MyGo's chosen segment lifts off the track with a one-DIP drop
        // shadow (`ui/toggle.go` coreSegmented: Shadow(0, 1, 2, 0, 12%)).
        // The node's face paints before its own commands, so the lifted
        // face is drawn here instead, shadow first.
        let lift = matches!(face, SegmentFace::Segment) && selected && !disabled;
        let node_ref = self.nodes.get_mut(&node).expect("segment node exists");
        node_ref.fill = if lift {
            None
        } else {
            fill.map(|color| (color, Radius::all(radius)))
        };
        node_ref.stroke = stroke.map(|color| (color, Radius::all(radius)));
        let mut commands = Vec::new();
        if lift {
            commands.push(DrawCommand::Shadow {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius,
                offset_x: 0.0,
                offset_y: 1.0,
                blur: 2.0,
                color: Color::rgba(0, 0, 0, 31),
            });
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(radius),
                color: theme.background,
            });
        }
        if let Some(icon_name) = icon {
            if let Some(icon) = Icon::builtin(icon_name) {
                // The icon is one em square, the size its box already
                // reserves; drawing it larger would spill past the padding.
                let icon_size = content_size.width.min(16.0);
                let icon_x = padding.left + (content_size.width - icon_size) / 2.0;
                let icon_y = padding.top + (content_size.height - icon_size) / 2.0;
                commands.push(DrawCommand::TransformPush {
                    transform: Transform {
                        translation: Point::new(icon_x, icon_y),
                        scale: Point::new(1.0, 1.0),
                        rotation: 0.0,
                    },
                });
                // MyGo dims a disabled element, and what is inside it, to
                // half its opacity (`ui/paint.go`), its icon with the rest.
                let color = if disabled {
                    mix_color(theme.muted, theme.background, 0.5)
                } else {
                    theme.foreground
                };
                commands.extend(icon.to_commands_with_direction(icon_size, color, self.direction));
                commands.push(DrawCommand::TransformPop);
            }
        } else if !label.is_empty() {
            let color = if disabled {
                mix_color(theme.muted, theme.background, 0.5)
            } else {
                theme.foreground
            };
            commands.push(DrawCommand::Text {
                x: padding.left,
                y: padding.top + (content_size.height - line) / 2.0,
                text: label.to_string(),
                font_size,
                weight: 400,
                italic: false,
                mono: false,
                family: None,
                color,
                max_width: Some(content_size.width.max(1.0)),
                alignment: TextAlignment::Start,
            });
        }
        if ring_visible {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        node_ref.commands = commands;
        self.register_access_node(
            node,
            Role::Button,
            label,
            Some(if selected { "active" } else { "inactive" }),
            focused,
        );
        SegmentState {
            clicked,
            hovered,
            pressed,
            focused,
        }
    }

    /// Creates a MyGo radio button in a row: a 16px dot and a label, in
    /// the text color either way (`ui/widgets.go` coreRadio).
    pub fn radio(
        &mut self,
        id: &str,
        label: &str,
        selected: &mut usize,
        value: usize,
    ) -> RadioState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let line = measured_text_height(font_size);
        let dot = 16.0;
        let gap = theme.spacing.sm;
        let height = line.max(dot);
        let width = dot + gap + measure_text_width(label, font_size);
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node, id.to_string());
        self.register_focusable(node, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let clicked = self.clicked_in_previous(id) || (focused && self.key_pressed(Key::Space));
        if clicked {
            *selected = value;
        }
        let on = *selected == value;
        let ring = if on || hovered {
            theme.accent
        } else {
            mix_color(theme.border, theme.foreground, 0.25)
        };
        let mut commands = Vec::new();
        let cy = height / 2.0;
        if on {
            commands.push(DrawCommand::Circle {
                x: dot / 2.0,
                y: cy,
                radius: 8.0,
                color: theme.accent,
            });
            // MyGo fills a Space(1.5)=6 diameter accent-text dot.
            commands.push(DrawCommand::Circle {
                x: dot / 2.0,
                y: cy,
                radius: 3.0,
                color: theme.on_accent,
            });
        } else {
            commands.push(DrawCommand::Circle {
                x: dot / 2.0,
                y: cy,
                radius: 8.0,
                color: ring,
            });
            commands.push(DrawCommand::Circle {
                x: dot / 2.0,
                y: cy,
                radius: 7.0,
                color: theme.background,
            });
        }
        commands.push(DrawCommand::Text {
            x: dot + gap,
            y: (height - line) / 2.0,
            text: label.to_string(),
            font_size,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: theme.foreground,
            max_width: Some((width - dot - gap).max(1.0)),
            alignment: TextAlignment::Start,
        });
        self.nodes
            .get_mut(&node)
            .expect("radio node exists")
            .commands = commands;
        self.register_access_node(
            node,
            Role::Radio,
            label,
            Some(if on { "selected" } else { "unselected" }),
            focused,
        );
        RadioState {
            clicked,
            hovered,
            focused,
        }
    }

    /// Creates an equal-width segmented selector.
    pub fn segmented(
        &mut self,
        id: &str,
        selected: &mut usize,
        labels: &[&str],
    ) -> SegmentedResponse {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let line_height = measured_text_height(font_size);
        if labels.is_empty() {
            *selected = 0;
        } else {
            *selected = (*selected).min(labels.len() - 1);
        }
        let segment_widths: Vec<f32> = labels
            .iter()
            .map(|label| measure_text_width(label, font_size) + 24.0)
            .collect();
        // MyGo's Segmented hugs its segments: one pixel of border and two
        // inside it, the segments two apart (`ui/toggle.go` coreSegmented).
        let width = segment_widths.iter().sum::<f32>()
            + 6.0
            + 2.0 * labels.len().saturating_sub(1) as f32;
        let height = line_height + 14.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                gap: 2.0,
                padding: Edges::all(3.0),
                ..Style::default()
            },
        );
        self.nodes.get_mut(&root).expect("segmented root exists").fill =
            Some((theme.surface, Radius::all(6.0)));
        self.nodes.get_mut(&root).expect("segmented root exists").stroke =
            Some((theme.border, Radius::all(6.0)));
        self.register_interaction(root, id.to_string());
        self.register_focusable(root, id.to_string());
        let hovered = self.pointer_in_previous(id);
        if hovered && self.input.pointer_down {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let mut clicked = None;
        for index in 0..labels.len() {
            let key = format!("{id}:segment:{index}");
            if self.clicked_in_previous(&key) {
                clicked = Some(index);
                self.focused_id = Some(id.to_string());
                break;
            }
        }
        let previous = *selected;
        if let Some(index) = clicked {
            *selected = index;
        }
        if focused {
            let next = if self.key_pressed(Key::ArrowRight) || self.key_pressed(Key::ArrowDown) {
                Some((*selected + 1).min(labels.len().saturating_sub(1)))
            } else if self.key_pressed(Key::ArrowLeft) || self.key_pressed(Key::ArrowUp) {
                Some(selected.saturating_sub(1))
            } else if self.key_pressed(Key::Home) {
                Some(0)
            } else if self.key_pressed(Key::End) {
                Some(labels.len().saturating_sub(1))
            } else {
                None
            };
            if let Some(next) = next {
                *selected = next;
            }
        }
        let changed = *selected != previous;
        if self.shows_focus_ring(focused) {
            let root_ref = self.nodes.get_mut(&root).expect("segmented root exists");
            root_ref.commands = vec![];
            root_ref.commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.push_parent(root);
        for (index, label) in labels.iter().enumerate() {
            let key = format!("{id}:segment:{index}");
            let segment_width = segment_widths[index];
            let segment_height = line_height + 8.0;
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(segment_width),
                    height: Some(segment_height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            let segment_hovered = self.pointer_in_previous(&key);
            let selected_segment = *selected == index;
            let mut commands = Vec::new();
            if selected_segment {
                // Raised off the track: white with a soft drop shadow.
                commands.push(DrawCommand::Shadow {
                    x: 0.0,
                    y: 0.0,
                    width: segment_width,
                    height: segment_height,
                    radius: 4.0,
                    offset_x: 0.0,
                    offset_y: 1.0,
                    blur: 2.0,
                    color: Color::rgba(0, 0, 0, 31),
                });
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: segment_width,
                    height: segment_height,
                    radius: Radius::all(4.0),
                    color: theme.background,
                });
            } else if segment_hovered {
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: segment_width,
                    height: segment_height,
                    radius: Radius::all(4.0),
                    color: theme.surface_hover,
                });
            }
            commands.push(DrawCommand::Text {
                x: 12.0,
                y: 4.0,
                text: label.to_string(),
                font_size,
                weight: 400,
                italic: false,
                mono: false,
                family: None,
                color: theme.foreground,
                max_width: Some((segment_width - 24.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
            self.register_access_node(
                node,
                Role::Tab,
                label,
                Some(if selected_segment {
                    "selected"
                } else {
                    "unselected"
                }),
                false,
            );
            self.nodes.get_mut(&node).expect("segment exists").commands = commands;
        }
        self.pop_parent();
        self.register_access_node(
            root,
            Role::TabList,
            id,
            Some(selected.to_string().as_str()),
            focused,
        );
        SegmentedResponse {
            selected: *selected,
            changed,
            clicked,
            focused,
            hovered,
        }
    }

    /// Creates a draggable button and paints the drag ghost.
    pub fn drag_source(
        &mut self,
        id: &str,
        label: &str,
        payload: &str,
        state: &mut DragState,
    ) -> DragSourceResponse {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = measure_text_width(label, font_size) + theme.spacing.md * 2.0;
        let height = 30.0;
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let mut started = false;
        if state.payload.is_none() && hovered && self.input.pointer_down {
            state.payload = Some(payload.to_string());
            state.origin = Some(id.to_string());
            state.current = self.input.pointer;
            started = true;
        }
        if state.payload.is_some() {
            if let Some(pointer) = self.input.pointer {
                state.current = Some(pointer);
            }
            if !self.input.pointer_down && self.input.released.is_none() {
                state.clear();
            }
        }
        let dragging = state.payload.is_some() && state.origin.as_deref() == Some(id);
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(5.0),
                color: if dragging || hovered {
                    mix_color(theme.surface, theme.accent, 0.18)
                } else {
                    theme.surface
                },
            },
            DrawCommand::Text {
                x: theme.spacing.md,
                y: 6.0,
                text: label.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: 500,
                color: theme.foreground,
                max_width: Some((width - theme.spacing.md * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            },
        ];
        if dragging {
            if let (Some(current), Some(anchor)) = (state.current, self.previous_regions.get(id)) {
                let ghost_width = 120.0;
                let ghost_height = 26.0;
                let ghost_x = current.x - anchor.bounds.x - ghost_width * 0.5;
                let ghost_y = current.y - anchor.bounds.y - ghost_height * 0.5;
                commands.extend([
                    DrawCommand::Rect {
                        x: ghost_x,
                        y: ghost_y,
                        width: ghost_width,
                        height: ghost_height,
                        color: mix_color(theme.accent, theme.surface, 0.65),
                    },
                    DrawCommand::Text {
                        x: ghost_x + 8.0,
                        y: ghost_y + 5.0,
                        text: state.payload.clone().unwrap_or_default(),
                        font_size: theme.font_sizes.small,
                        italic: false,
                        mono: false,
                        family: None,
                        weight: 500,
                        color: theme.on_accent,
                        max_width: Some(ghost_width - 16.0),
                        alignment: TextAlignment::Start,
                    },
                ]);
            }
        }
        self.register_access_node(node, Role::Button, label, Some("drag source"), false);
        self.nodes
            .get_mut(&node)
            .expect("drag source exists")
            .commands = commands;
        DragSourceResponse { started, dragging }
    }

    /// Creates a drop target that consumes the active drag on release.
    pub fn drop_target(
        &mut self,
        id: &str,
        label: &str,
        state: &mut DragState,
    ) -> DropTargetResponse {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = measure_text_width(label, font_size) + theme.spacing.md * 2.0;
        let height = 34.0;
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let drag_active = state.payload.is_some();
        let mut dropped = None;
        if drag_active && self.clicked_in_previous(id) {
            if let (Some(payload), Some(origin)) = (state.payload.clone(), state.origin.clone()) {
                dropped = Some(DragDrop {
                    target: id.to_string(),
                    origin,
                    payload,
                });
                state.clear();
            }
        }
        self.nodes
            .get_mut(&node)
            .expect("drop target exists")
            .commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(6.0),
                color: if hovered && drag_active {
                    mix_color(theme.accent, theme.surface, 0.25)
                } else {
                    theme.surface
                },
            },
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(9.0),
                color: if hovered && drag_active {
                    theme.accent
                } else {
                    theme.border
                },
            },
            DrawCommand::RoundedRect {
                x: 2.0,
                y: 2.0,
                width: width - 4.0,
                height: height - 4.0,
                radius: Radius::all(5.0),
                color: theme.surface,
            },
            DrawCommand::Text {
                x: theme.spacing.md,
                y: 8.0,
                text: label.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: 500,
                color: theme.foreground,
                max_width: Some((width - theme.spacing.md * 2.0).max(1.0)),
                alignment: TextAlignment::Center,
            },
        ];
        self.register_access_node(node, Role::Button, label, Some("drop target"), false);
        DropTargetResponse { hovered, dropped }
    }

    /// Creates a simple draggable reorder list.
    pub fn reorder_list(
        &mut self,
        id: &str,
        state: &mut ListReorderState,
        rows: &[ListRow<'_>],
        row_height: f32,
    ) -> ListReorderResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.xs,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let mut dropped = false;
        let mut from = None;
        let mut to = None;
        for (index, row) in rows.iter().enumerate() {
            let key = format!("{id}:row:{index}");
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(width),
                    height: Some(row_height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            let hovered = self.pointer_in_previous(&key);
            if state.drag.payload.is_none() && hovered && self.input.pointer_down {
                state.drag.payload = Some(index.to_string());
                state.drag.origin = Some(id.to_string());
                state.drag.current = self.input.pointer;
            }
            if state.drag.payload.is_some() {
                if let Some(pointer) = self.input.pointer {
                    state.drag.current = Some(pointer);
                }
            }
            if state.drag.payload.is_some() && self.clicked_in_previous(&key) {
                if let Some(payload) = state.drag.payload.clone() {
                    if let Ok(source) = payload.parse::<usize>() {
                        from = Some(source);
                        to = Some(index);
                        dropped = true;
                        state.from = Some(source);
                        state.to = Some(index);
                        state.drag.clear();
                    }
                }
            }
            let active = state.drag.payload.is_some();
            let source = state.drag.payload.as_deref() == Some(index.to_string().as_str());
            self.nodes.get_mut(&node).expect("list row exists").commands = vec![
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height: row_height,
                    radius: Radius::all(3.0),
                    color: if source {
                        mix_color(theme.accent, theme.surface, 0.25)
                    } else if hovered && active {
                        mix_color(theme.accent, theme.surface, 0.12)
                    } else {
                        theme.surface
                    },
                },
                DrawCommand::Text {
                    x: 8.0,
                    y: 3.0,
                    text: row.label.to_string(),
                    font_size: theme.font_sizes.small,
                    italic: false,
                    mono: false,
                    family: None,
                    weight: 400,
                    color: theme.foreground,
                    max_width: Some(width - 16.0),
                    alignment: TextAlignment::Start,
                },
            ];
            self.register_access_node(node, Role::Button, row.label, None, false);
        }
        self.pop_parent();
        self.register_access_node(root, Role::Group, id, None, false);
        if state.drag.payload.is_some() && !self.input.pointer_down && self.input.released.is_none()
        {
            state.drag.clear();
        }
        ListReorderResponse { from, to, dropped }
    }

    /// Creates a line diff viewer.
    pub fn diff_viewer(
        &mut self,
        id: &str,
        old: &str,
        new: &str,
        state: &mut DiffViewerState,
    ) -> DiffViewerResponse {
        let theme = self.theme.clone();
        // The filler's stripes are the text color at 7% over the page
        // (`ui/editor/diffside.go` fillerStripes, `ct.Text.Alpha(0.07)`).
        let hatch = Color::rgba(theme.foreground.r, theme.foreground.g, theme.foreground.b, 18);
        let width = self.current_content_width();
        let height = self.current_content_height();
        let old_lines = split_lines(old);
        let new_lines = split_lines(new);
        let hunks = lines_diff(&old_lines, &new_lines);
        let added: usize = hunks.iter().map(|h| h.b1 - h.b0).sum();
        let removed: usize = hunks.iter().map(|h| h.a1 - h.a0).sum();
        let same = old_lines.len() + new_lines.len() - added - removed;
        let toggle_key = format!("{id}:layout");
        if self.clicked_in_previous(&toggle_key) {
            state.layout = match state.layout {
                DiffLayout::Unified => DiffLayout::SideBySide,
                DiffLayout::SideBySide => DiffLayout::Unified,
            };
        }
        // MyGo's diff viewer: an editor on each side of a split view, both
        // gutters as wide, tinted changed lines, striped bars for deletions,
        // word highlights within changed lines, and the change marks of the
        // new side's scroll bar (`ui/editor/diffview.go`, `diffside.go`).
        let code_size = 13.0_f32;
        let line_height = 19.5_f32;
        let char_width = host_text_width_weighted("0", code_size, false, true, 400)
            .unwrap_or(code_size * 0.6);
        let ascent = host_text_ascent(code_size, false, true, false).unwrap_or(code_size * 0.93);
        let descent = host_text_descent(code_size, false, true, false).unwrap_or(code_size * 0.25);
        let line_top = ((line_height - ascent - descent) / 2.0).max(0.0);
        // Both gutters take the digits of the longer text, at least three,
        // then the change bar's room and the padding (`gutterWidth`).
        let digits = 3.max(old_lines.len().max(new_lines.len()).to_string().len());
        let gutter = (digits as f32 * char_width + char_width * 1.5 + 4.0 + 6.0).ceil();
        let text_x = gutter + 4.0;
        let geometry = DiffGeometry {
            gutter,
            text_x,
            line_height,
            line_top,
            code_size,
            scrollbar: 10.0,
        };
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
        );
        let mut commands = Vec::new();
        let rows = split_rows(&old_lines, &new_lines, &hunks);
        let half = ((width - 1.0) / 2.0).max(0.0);
        commands.push(DrawCommand::Rect {
            x: half,
            y: 0.0,
            width: 1.0,
            height,
            color: DIVIDER,
        });
        let hunk_of = |line: usize, old: bool| -> Option<usize> {
            hunks.iter().position(|h| {
                if old {
                    line >= h.a0 && line < h.a1
                } else {
                    line >= h.b0 && line < h.b1
                }
            })
        };
        // The rows a side lacks, hatched as one filler per run.
        for side in 0..2 {
            let mut row = 0;
            while row < rows.len() {
                let missing = if side == 0 {
                    rows[row].old.is_none()
                } else {
                    rows[row].new.is_none()
                };
                if !missing {
                    row += 1;
                    continue;
                }
                let start = row;
                while row < rows.len() {
                    let missing = if side == 0 {
                        rows[row].old.is_none()
                    } else {
                        rows[row].new.is_none()
                    };
                    if !missing {
                        break;
                    }
                    row += 1;
                }
                let x = if side == 0 { 0.0 } else { half + 1.0 };
                let side_width = if side == 0 { half } else { width - half - 1.0 };
                let y = start as f32 * line_height;
                let run = (row - start) as f32 * line_height;
                push_filler(&mut commands, x, y, side_width, run, gutter, hatch);
            }
        }
        for (row, pair) in rows.iter().enumerate() {
            let y = row as f32 * line_height;
            if y >= height {
                break;
            }
            for side in 0..2 {
                let old_side = side == 0;
                let x = if old_side { 0.0 } else { half + 1.0 };
                let side_width = if old_side { half } else { width - half - 1.0 };
                let line = if old_side { pair.old } else { pair.new };
                let Some(index) = line else {
                    continue;
                };
                let hunk = hunk_of(index, old_side);
                let look = match (old_side, hunk) {
                    (true, Some(_)) => LineLook::removed(),
                    (false, Some(_)) => LineLook::added(),
                    _ => LineLook::same(),
                };
                let text = expand_tabs(if old_side {
                    &old_lines[index]
                } else {
                    &new_lines[index]
                });
                // Words changed within the line, compared with the line the
                // other side pairs with.
                let words = match (hunk, pair.old, pair.new) {
                    (Some(_), Some(a), Some(b)) => {
                        let raw = if old_side { &old_lines[a] } else { &new_lines[b] };
                        let (ra, rb) = word_ranges(&old_lines[a], &new_lines[b]);
                        let ranges = if old_side { ra } else { rb };
                        ranges
                            .into_iter()
                            .map(|r| {
                                (
                                    expand_tabs_prefix(raw, r.start).len(),
                                    expand_tabs_prefix(raw, r.end).len(),
                                )
                            })
                            .collect()
                    }
                    _ => Vec::new(),
                };
                self.push_diff_line(
                    &mut commands,
                    x,
                    y,
                    side_width,
                    &text,
                    Some(index + 1),
                    &look,
                    &geometry,
                    &theme,
                    &words,
                );
            }
        }
        // The changes on the new side's scroll bar, as MyGo marks them.
        let total = new_lines.len().max(1) as f32;
        for hunk in &hunks {
            if hunk.b0 == hunk.b1 {
                continue;
            }
            let color = if hunk.a0 == hunk.a1 {
                ADDED_BAR
            } else {
                MODIFIED
            };
            let from = (hunk.b0 as f32 / total * height).round();
            let to = (hunk.b1 as f32 / total * height).round();
            commands.push(DrawCommand::Rect {
                x: width - 10.0,
                y: from,
                width: 3.0,
                height: (to - from).max(2.0),
                color,
            });
        }
        self.nodes
            .get_mut(&root)
            .expect("diff root exists")
            .commands = commands;
        self.register_access_node(root, Role::Group, id, None, false);
        DiffViewerResponse {
            lines: diff_lines(old, new),
            added,
            removed,
            same,
            layout: state.layout,
        }
    }

    /// Draws one diff line at `x`, `y`, as MyGo's editor paints a line: the
    /// tint across the text, the gutter's, the change bar, the number, and
    /// the words changed within it.
    #[allow(clippy::too_many_arguments)]
    fn push_diff_line(
        &self,
        commands: &mut Vec<DrawCommand>,
        x: f32,
        y: f32,
        width: f32,
        text: &str,
        number: Option<usize>,
        look: &LineLook,
        geometry: &DiffGeometry,
        theme: &Theme,
        words: &[(usize, usize)],
    ) {
        // The tint runs under the scroll bar's track too: MyGo paints it
        // across the text rectangle, which the track lies over.
        let text_width = (width - geometry.gutter).max(0.0);
        if let Some(color) = look.bg {
            commands.push(DrawCommand::Rect {
                x: x + geometry.gutter,
                y,
                width: text_width,
                height: geometry.line_height,
                color,
            });
        }
        if let Some(color) = look.gutter {
            commands.push(DrawCommand::Rect {
                x,
                y,
                width: geometry.gutter,
                height: geometry.line_height,
                color,
            });
        }
        if let Some(color) = look.bar {
            if look.striped {
                // Two DIPs every four from the content's top, cut to the
                // line, so consecutive lines make one pattern that scrolls
                // with them (`ui/editor/paint.go` barStripes).
                const PERIOD: f32 = 4.0;
                const WIDTH: f32 = 2.0;
                let phase = y.rem_euclid(PERIOD);
                let mut stripe = y - phase;
                while stripe < y + geometry.line_height {
                    let top = stripe.max(y);
                    let bottom = (stripe + WIDTH).min(y + geometry.line_height);
                    if bottom > top {
                        commands.push(DrawCommand::Rect {
                            x,
                            y: top,
                            width: 4.0,
                            height: bottom - top,
                            color,
                        });
                    }
                    stripe += PERIOD;
                }
            } else {
                commands.push(DrawCommand::Rect {
                    x,
                    y,
                    width: 4.0,
                    height: geometry.line_height,
                    color,
                });
            }
        }
        if let Some(n) = number {
            let digits = n.to_string();
            let number_width =
                host_text_width_weighted(&digits, geometry.code_size, false, true, 400)
                    .unwrap_or(geometry.code_size * 0.6 * digits.len() as f32);
            commands.push(DrawCommand::Text {
                x: x + geometry.gutter - 4.0 - geometry.code_size * 0.3 - number_width,
                y: y + geometry.line_top,
                text: digits,
                font_size: geometry.code_size,
                italic: false,
                mono: true,
                family: None,
                weight: 400,
                color: look.number.unwrap_or(LINE_NUMBER),
                max_width: Some(number_width.max(1.0)),
                alignment: TextAlignment::Start,
            });
        }
        let origin = x + geometry.text_x;
        let clip = Rect {
            x: x + geometry.gutter,
            y,
            width: text_width,
            height: geometry.line_height,
        };
        commands.push(DrawCommand::ScissorPush { rect: clip });
        // The words changed within the line, at 22% of the change color, as
        // MyGo's `lineLook.word` highlights them.
        if let Some(bar) = look.bar {
            let word = Color::rgba(bar.r, bar.g, bar.b, 56);
            for (start, end) in words {
                let (start, end) = (*start, (*end).min(text.len()));
                if end <= start || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
                    continue;
                }
                let before = host_text_width_weighted(
                    &text[..start],
                    geometry.code_size,
                    false,
                    true,
                    400,
                )
                .unwrap_or(geometry.code_size * 0.6 * start as f32);
                let span =
                    host_text_width_weighted(&text[start..end], geometry.code_size, false, true, 400)
                        .unwrap_or(geometry.code_size * 0.6 * (end - start) as f32);
                commands.push(DrawCommand::RoundedRect {
                    x: origin + before,
                    y,
                    width: span,
                    height: geometry.line_height,
                    radius: Radius::all(3.0),
                    color: word,
                });
            }
        }
        self.push_line_tokens(
            commands,
            text,
            origin,
            y + geometry.line_top,
            geometry.code_size,
            theme,
        );
        commands.push(DrawCommand::ScissorPop);
    }

    /// Appends the tokens of one line of Go, colored as MyGo's editor
    /// colors them, and returns the width they take.
    fn push_line_tokens(
        &self,
        commands: &mut Vec<DrawCommand>,
        line: &str,
        start_x: f32,
        y: f32,
        code_size: f32,
        theme: &Theme,
    ) -> f32 {
        let mut x = start_x;
        for (token_text, token_kind) in go_tokens(line) {
            let token_width = host_text_width_weighted(&token_text, code_size, false, true, 400)
                .unwrap_or_else(|| measure_text_width(&token_text, code_size));
            let color = match token_kind {
                CodeColor::Keyword | CodeColor::Operator => Color::rgb(207, 34, 46),
                CodeColor::Type => Color::rgb(149, 56, 0),
                CodeColor::Constant => Color::rgb(5, 80, 174),
                CodeColor::Function => Color::rgb(130, 80, 223),
                CodeColor::Property | CodeColor::Number => Color::rgb(5, 80, 174),
                CodeColor::String => Color::rgb(10, 48, 105),
                CodeColor::Comment => Color::rgb(110, 119, 129),
                CodeColor::Text => theme.foreground,
            };
            commands.push(DrawCommand::Text {
                x,
                y,
                text: token_text,
                font_size: code_size,
                italic: false,
                mono: true,
                family: None,
                weight: 400,
                color,
                max_width: Some(token_width.max(1.0)),
                alignment: TextAlignment::Start,
            });
            x += token_width;
        }
        x - start_x
    }

    pub fn code_editor(
        &mut self,
        id: &str,
        code: &str,
        state: &mut CodeEditorState,
    ) -> CodeEditorResponse {
        self.code_editor_with_options(id, code, state, &CodeEditorOptions::default())
    }

    /// Creates a read-only code viewer with MyGo's editor options.
    ///
    /// The geometry follows `ui/editor` in MyGo: the gutter holds the digits
    /// plus one and a half characters, a fold marker's slot and a pad of 4
    /// (`gutterWidth`), tabs advance to the next stop (`view.go` expand), and
    /// the line with the caret carries the active-line fill.
    pub fn code_editor_with_options(
        &mut self,
        id: &str,
        code: &str,
        state: &mut CodeEditorState,
        options: &CodeEditorOptions<'_>,
    ) -> CodeEditorResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        // MyGo's editor defaults to a 13-pixel mono face with a line box of
        // 1.5 of it, rounded at the device scale (`ui/editor/view.go`
        // setStyle): 19.5 at a 2x device.
        let code_size = 13.0_f32;
        let line_height = (code_size * 1.5 * 2.0).round() / 2.0;
        // A buffer's lines are its newline-separated lines: text ending in
        // a newline has an empty last line, and MyGo's editor shows it.
        let lines = code.split('\n').collect::<Vec<_>>();
        let line_count = lines.len().max(1);
        let previous = state.selected_line;
        let mut clicked_line = None;
        for index in 0..lines.len() {
            let key = format!("{id}:line:{index}");
            if self.clicked_in_previous(&key) {
                state.selected_line = Some(index);
                clicked_line = Some(index);
                self.focused_id = Some(id.to_string());
                break;
            }
        }
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
        );
        self.register_interaction(root, id.to_string());
        self.register_focusable(root, id.to_string());
        let focused = self.focused_id.as_deref() == Some(id);
        let height = line_count as f32 * line_height;
        let mut root_commands = Vec::new();
        if self.shows_focus_ring(focused) {
            root_commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.nodes
            .get_mut(&root)
            .expect("code root exists")
            .commands = root_commands;
        self.push_parent(root);
        // MyGo's gutter: the digits, one and a half characters of room, the
        // fold marker's slot and a pad of 4, rounded up (`gutterWidth` in
        // `ui/editor/editor.go`). It reserves at least three digits
        // (`State.options` sets `minDigits: 3`).
        let char_width = host_text_width_weighted("0", code_size, false, true, 400)
            .unwrap_or(code_size * 0.6);
        let digits = (line_count.to_string().len()).max(3) as f32;
        let fold_slot = line_height * 0.8;
        let gutter = (char_width * digits + char_width * 1.5 + fold_slot + 4.0).ceil();
        let text_x = gutter + 4.0;
        // Line numbers end a pad of 4 and half a character short of the
        // gutter's right edge, before the fold marker's slot (`paintGutter`).
        let number_right = gutter - 4.0 - char_width * 0.5 - fold_slot;
        // MyGo centers the line's glyphs in its line box and rounds the
        // baseline to a whole pixel (`ui/editor/view.go` paintRow, the canvas
        // engine's baseline).
        let ascent = host_text_ascent(code_size, false, true, false).unwrap_or(code_size * 0.9);
        let descent = host_text_descent(code_size, false, true, false).unwrap_or(code_size * 0.25);
        let line_top = (line_height - ascent - descent) / 2.0;
        // The caret's line carries the active-line fill, over the text
        // column only, and its number takes the text color; neither waits for
        // the focus, as MyGo's `activeLines` comes from the selections alone
        // (`ui/editor/paint.go`). An editor always has a caret, at its first
        // line until one is clicked.
        let caret_line = Some(state.selected_line.unwrap_or(0));
        let active_fill = Color::rgba(0, 0, 0, 9);
        let guide_color = Color::rgba(theme.border.r, theme.border.g, theme.border.b, 140);
        let wrap_guide_color = Color::rgba(theme.border.r, theme.border.g, theme.border.b, 153);
        let tab_size = options.tab_size.max(1);
        for (index, line) in lines.iter().enumerate() {
            let key = format!("{id}:line:{index}");
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(width),
                    height: Some(line_height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            let hovered = self.pointer_in_previous(&key);
            let selected = state.selected_line == Some(index);
            let active = caret_line == Some(index);
            let mut commands = Vec::new();
            if hovered && !active {
                commands.push(DrawCommand::Rect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height: line_height,
                    color: mix_color(theme.background, theme.foreground, 0.03),
                });
            }
            if active {
                commands.push(DrawCommand::Rect {
                    x: gutter,
                    y: 0.0,
                    width: (width - gutter).max(0.0),
                    height: line_height,
                    color: active_fill,
                });
            }
            // The line's indentation, in columns, as its guides are drawn
            // at: a blank line takes the larger of the lines around it
            // (`lineIndent` in `ui/editor/paint.go`).
            let indent = line_indent(&lines, index, tab_size);
            let guide_height = line_height;
            for level in 0..(indent / tab_size) {
                let x = (text_x + level as f32 * tab_size as f32 * char_width).round();
                commands.push(DrawCommand::Rect {
                    x,
                    y: 0.0,
                    width: 1.0,
                    height: guide_height,
                    color: guide_color,
                });
            }
            if let Some(column) = options.wrap_guide {
                let x = (text_x + column as f32 * char_width).round();
                commands.push(DrawCommand::Rect {
                    x,
                    y: 0.0,
                    width: 1.0,
                    height: guide_height,
                    color: wrap_guide_color,
                });
            }
            // The number sits at the gutter's right edge, in the text color
            // on the caret's line and the muted line-number color elsewhere
            // (`paintGutter`).
            let number = (index + 1).to_string();
            let number_width = host_text_width_weighted(&number, code_size, false, true, 400)
                .unwrap_or_else(|| measure_text_width(&number, code_size));
            commands.push(DrawCommand::Text {
                x: (number_right - number_width).max(0.0),
                y: line_top,
                text: number,
                font_size: code_size,
                italic: false,
                mono: true,
                family: None,
                weight: 400,
                color: if active {
                    theme.foreground
                } else {
                    Color::rgba(theme.muted.r, theme.muted.g, theme.muted.b, 191)
                },
                max_width: Some(number_width.max(1.0)),
                alignment: TextAlignment::Start,
            });
            // Tabs advance to the next stop, as MyGo expands them before
            // laying a line out (`view.go` expand).
            let mut x = text_x;
            let mut column = 0_usize;
            for (token_text, token_kind) in go_token_kinds(line) {
                let color = match token_kind {
                    CodeColor::Keyword | CodeColor::Operator => Color::rgb(207, 34, 46),
                    CodeColor::Type => Color::rgb(149, 56, 0),
                    CodeColor::Constant => Color::rgb(5, 80, 174),
                    CodeColor::Function => Color::rgb(130, 80, 223),
                    CodeColor::Property | CodeColor::Number => Color::rgb(5, 80, 174),
                    CodeColor::String => Color::rgb(10, 48, 105),
                    CodeColor::Comment => Color::rgb(110, 119, 129),
                    CodeColor::Text => theme.foreground,
                };
                let mut run = String::new();
                let mut run_x = x;
                for character in token_text.chars() {
                    if character == '\t' {
                        if !run.is_empty() {
                            let run_width =
                                host_text_width_weighted(&run, code_size, false, true, 400)
                                    .unwrap_or_else(|| measure_text_width(&run, code_size));
                            commands.push(DrawCommand::Text {
                                x: run_x,
                                y: line_top,
                                text: std::mem::take(&mut run),
                                font_size: code_size,
                                italic: false,
                                mono: true,
                                family: None,
                                weight: 400,
                                color,
                                max_width: Some(run_width.max(1.0)),
                                alignment: TextAlignment::Start,
                            });
                        }
                        let advance = tab_size - column % tab_size;
                        column += advance;
                        x += advance as f32 * char_width;
                        run_x = x;
                        continue;
                    }
                    column += 1;
                    run.push(character);
                }
                if !run.is_empty() {
                    let run_width = host_text_width_weighted(&run, code_size, false, true, 400)
                        .unwrap_or_else(|| measure_text_width(&run, code_size));
                    commands.push(DrawCommand::Text {
                        x: run_x,
                        y: line_top,
                        text: run,
                        font_size: code_size,
                        italic: false,
                        mono: true,
                        family: None,
                        weight: 400,
                        color,
                        max_width: Some(run_width.max(1.0)),
                        alignment: TextAlignment::Start,
                    });
                }
                x = text_x + column as f32 * char_width;
            }
            // Diagnostics rule a wavy line under their characters, two and a
            // half pixels above the line box's bottom (`squiggle` in
            // `ui/editor/paint.go`).
            for diagnostic in options.diagnostics.iter().filter(|d| d.line == index) {
                let start = diagnostic.start.min(line.chars().count());
                let end = diagnostic.end.min(line.chars().count()).max(start);
                let from = text_x + display_column(line, start, tab_size) as f32 * char_width;
                let to = text_x + display_column(line, end, tab_size) as f32 * char_width;
                let color = match diagnostic.severity {
                    CodeSeverity::Error => Color::rgb(209, 36, 47),
                    CodeSeverity::Warning => Color::rgb(154, 103, 0),
                    CodeSeverity::Info => Color::rgb(9, 105, 218),
                    CodeSeverity::Hint => Color::rgb(110, 119, 129),
                };
                commands.push(DrawCommand::SvgPath {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height: line_height,
                    path: squiggle_path(from, line_height - 2.5, (to - from).max(0.0)),
                    fill: None,
                    stroke: Some(color),
                    stroke_width: 1.0,
                });
            }
            self.register_access_node(
                node,
                Role::Label,
                format!("line {}", index + 1).as_str(),
                Some(if selected { "selected" } else { "unselected" }),
                false,
            );
            self.nodes
                .get_mut(&node)
                .expect("code line exists")
                .commands = commands;
        }
        self.pop_parent();
        self.register_access_node(root, Role::Group, id, None, focused);
        CodeEditorResponse {
            selected_line: state.selected_line,
            clicked_line,
            line_count: lines.len(),
            changed: state.selected_line != previous,
        }
    }

    /// Creates a 6x7 month calendar.
    pub fn calendar(&mut self, id: &str, state: &mut CalendarState) -> CalendarResponse {
        // MyGo's Calendar (`ui/date.go`): a shrink-to-fit column with
        // Space(0.5) between its rows, no background. A header row holds a
        // chevron button either side of the month and year; then a row of
        // weekday initials, then six rows of days, each cell Space(8) wide
        // and Space(7) tall, centered, radius Radius. The chosen day fills
        // with the accent and its text turns AccentText; the cursor day
        // fills with the hover surface; today borders in the accent.
        let theme = self.theme.clone();
        state.month = state.month.clamp(1, 12);
        let days = days_in_month(state.year, state.month);
        if state.selected.is_none() {
            state.selected = Some(1);
        }
        if let Some(selected) = state.selected {
            state.selected = Some(selected.clamp(1, days));
        }
        let previous_year = state.year;
        let previous_month = state.month;
        let previous_selected = state.selected;
        let previous_key = format!("{id}:previous");
        let next_key = format!("{id}:next");
        let mut changed = false;
        if self.clicked_in_previous(&previous_key) {
            let (year, month) = move_month(state.year, state.month, -1);
            state.year = year;
            state.month = month;
            state.selected = Some(state.selected.unwrap_or(1).min(days_in_month(year, month)));
            changed = true;
        }
        if self.clicked_in_previous(&next_key) {
            let (year, month) = move_month(state.year, state.month, 1);
            state.year = year;
            state.month = month;
            state.selected = Some(state.selected.unwrap_or(1).min(days_in_month(year, month)));
            changed = true;
        }
        let mut clicked = None;
        for day in 1..=days_in_month(state.year, state.month) {
            let key = format!("{id}:day:{day}");
            if self.clicked_in_previous(&key) {
                state.selected = Some(day);
                clicked = Some(day);
                self.focused_id = Some(id.to_string());
                changed = true;
                break;
            }
        }
        let focused = self.focused_id.as_deref() == Some(id);
        if focused {
            let mut selected = state.selected.unwrap_or(1);
            if self.key_pressed(Key::PageUp) {
                let (year, month) = move_month(state.year, state.month, -1);
                state.year = year;
                state.month = month;
                selected = selected.min(days_in_month(year, month));
                changed = true;
            }
            if self.key_pressed(Key::PageDown) {
                let (year, month) = move_month(state.year, state.month, 1);
                state.year = year;
                state.month = month;
                selected = selected.min(days_in_month(year, month));
                changed = true;
            }
            let mut delta = 0_i64;
            if self.key_pressed(Key::ArrowLeft) {
                delta -= 1;
            }
            if self.key_pressed(Key::ArrowRight) {
                delta += 1;
            }
            if self.key_pressed(Key::ArrowUp) {
                delta -= 7;
            }
            if self.key_pressed(Key::ArrowDown) {
                delta += 7;
            }
            if delta != 0 {
                let serial = days_from_civil(state.year, state.month, selected);
                let (year, month, day) = civil_from_days(serial + delta);
                state.year = year;
                state.month = month;
                selected = day;
                changed = true;
            }
            if self.key_pressed(Key::Home) {
                selected = 1;
                changed = true;
            }
            if self.key_pressed(Key::End) {
                selected = days_in_month(state.year, state.month);
                changed = true;
            }
            state.selected = Some(selected);
        }
        let cell_width = theme.spacing.base * 8.0;
        let cell_height = theme.spacing.base * 7.0;
        // The grid is seven cells wide, and every row stretches to it
        // (`ui/date.go` calendarBox, calendarGrid).
        let grid_width = cell_width * 7.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(grid_width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.base * 0.5,
                ..Style::default()
            },
        );
        self.register_interaction(root, id.to_string());
        self.register_focusable(root, id.to_string());
        self.push_parent(root);
        self.row()
            .gap(theme.spacing.base)
            .height(20.0)
            .align(AlignItems::Center)
            .children(|context| {
                // MyGo's chevron is a themed button, Space(1) padded above
                // and below and Space(2.5) at the sides around a 1.5-by-2.5
                // stroke, inside its one-pixel border: 28 by 20
                // (`ui/date.go` monthChevron, `ui/widgets.go` styleButton).
                let chevron = |context: &mut Context, key: &str, label: &str, back: bool| {
                    let button = context.new_named_node(
                        key,
                        Style {
                            width: Some(theme.spacing.base * 7.0),
                            height: Some(theme.spacing.base * 5.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..Style::default()
                        },
                    );
                    context.register_interaction(button, key.to_string());
                    // MyGo's month chevron is a 1.5-wide, 2.5-tall stroke:
                    // back points left, forth right (`ui/date.go`
                    // monthChevron).
                    let (tip, ends) = if back { (0.15_f32, 0.85_f32) } else { (0.85, 0.15) };
                    let width = theme.spacing.base * 1.5;
                    let height = theme.spacing.base * 2.5;
                    context
                        .nodes
                        .get_mut(&button)
                        .expect("calendar chevron exists")
                        .commands = vec![
                        DrawCommand::RoundedRect {
                            x: 1.0,
                            y: 1.0,
                            width: theme.spacing.base * 7.0 - 2.0,
                            height: theme.spacing.base * 5.0 - 2.0,
                            radius: Radius::all(5.0),
                            color: theme.surface,
                        },
                        rounded_ring(
                            theme.spacing.base * 7.0,
                            theme.spacing.base * 5.0,
                            6.0,
                            1.0,
                            theme.border,
                        ),
                        DrawCommand::SvgPath {
                            x: (theme.spacing.base * 7.0 - width) / 2.0,
                            y: (theme.spacing.base * 5.0 - height) / 2.0,
                            width,
                            height,
                            path: format!(
                                "M{} {}L{} {}L{} {}",
                                width * ends,
                                height * 0.1,
                                width * tip,
                                height * 0.5,
                                width * ends,
                                height * 0.9
                            ),
                            fill: None,
                            stroke: Some(theme.foreground),
                            stroke_width: 1.5,
                        },
                    ];
                    context.register_access_node(button, Role::Button, label, None, false);
                };
                chevron(context, &previous_key, "Previous month", true);
                // The title takes what the chevrons leave and centers in
                // it (`ui/date.go` calendarGrid: coreText(title).Grow(1)
                // .TextAlign(Center)).
                let title_width = grid_width - theme.spacing.base * 14.0 - theme.spacing.base * 2.0;
                context
                    .text(&format!("{} {}", month_name(state.month), state.year))
                    .bold()
                    .color(theme.foreground)
                    .align(TextAlignment::Center)
                    .max_width(title_width)
                    .width(title_width);
                chevron(context, &next_key, "Next month", false);
            });
        // MyGo prints CLDR's short weekday names, "Mo" through "Su"
        // (`i18n/cldr.go` Weekdays with Short, `website/docs-demo/dates.go`).
        self.row().height(15.0).children(|context| {
            for (index, label) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].iter().enumerate() {
                let _ = index;
                // MyGo gives each weekday cell Space(8) of width and
                // centers its text (`ui/date.go` calendarGrid).
                context
                    .text(label)
                    .font_size(theme.font_sizes.body - 2.0)
                    .color(theme.muted)
                    .align(TextAlignment::Center)
                    .max_width(cell_width)
                    .width(cell_width);
            }
        });
        let leading = weekday_monday(state.year, state.month, 1) as usize;
        let days = days_in_month(state.year, state.month);
        // MyGo fills the six weeks around the month, so the days of the
        // months either side show too, muted (`ui/date.go` calendarGrid).
        let previous_days = {
            let (year, month) = move_month(state.year, state.month, -1);
            days_in_month(year, month)
        };
        let next_days = {
            let (year, month) = move_month(state.year, state.month, 1);
            days_in_month(year, month)
        };
        for row in 0..6 {
            self.row().height(cell_height).children(|context| {
                for column in 0..7 {
                    let position = row * 7 + column;
                    let offset = position as i32 - leading as i32;
                    // The day's number, and whether it belongs to a month
                    // other than the one shown.
                    let (day, outside) = if offset < 0 {
                        (previous_days as i32 + offset + 1, true)
                    } else if offset >= days as i32 {
                        (offset - days as i32 + 1, true)
                    } else {
                        (offset + 1, false)
                    };
                    let key = if outside {
                        format!("{id}:outside:{position}")
                    } else {
                        format!("{id}:day:{day}")
                    };
                    let node = context.new_named_node(
                        &key,
                        Style {
                            width: Some(cell_width),
                            height: Some(cell_height),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..Style::default()
                        },
                    );
                    if !outside {
                        context.register_interaction(node, key.clone());
                    }
                    let hovered = !outside && context.pointer_in_previous(&key);
                    let selected = !outside && state.selected == Some(day as u32);
                    let mut commands = Vec::new();
                    if selected {
                        commands.push(DrawCommand::RoundedRect {
                            x: 0.0,
                            y: 0.0,
                            width: cell_width,
                            height: cell_height,
                            radius: Radius::all(6.0),
                            color: theme.accent,
                        });
                    } else if hovered {
                        commands.push(DrawCommand::RoundedRect {
                            x: 0.0,
                            y: 0.0,
                            width: cell_width,
                            height: cell_height,
                            radius: Radius::all(6.0),
                            color: theme.surface_hover,
                        });
                    }
                    context.register_access_node(
                        node,
                        Role::Button,
                        &format!("{:04}-{:02}-{:02}", state.year, state.month, day),
                        Some(if selected { "selected" } else { "unselected" }),
                        false,
                    );
                    context
                        .nodes
                        .get_mut(&node)
                        .expect("calendar day exists")
                        .commands = commands;
                    context.push_parent(node);
                    context.text(&day.to_string()).color(if selected {
                        theme.on_accent
                    } else if outside {
                        theme.muted
                    } else {
                        theme.foreground
                    });
                    context.pop_parent();
                }
            });
        }
        self.pop_parent();
        self.register_access_node(
            root,
            Role::Group,
            id,
            Some(
                format!(
                    "{:04}-{:02}-{:02}",
                    state.year,
                    state.month,
                    state.selected.unwrap_or(0)
                )
                .as_str(),
            ),
            focused,
        );
        CalendarResponse {
            year: state.year,
            month: state.month,
            selected: state.selected,
            changed: changed
                || state.year != previous_year
                || state.month != previous_month
                || state.selected != previous_selected,
            clicked,
        }
    }

    /// Creates a date input showing the date in the locale's short form,
    /// with a calendar icon at its end that opens a calendar, as MyGo's
    /// `coreDateInput` (`ui/date.go`): a button styled field, its text at
    /// the start and the icon at the end.
    pub fn date_input(
        &mut self,
        id: &str,
        text: &mut String,
        state: &mut DateInputState,
    ) -> DateInputResponse {
        let theme = self.theme.clone();
        let space = theme.spacing.base;
        let font_size = theme.font_sizes.body;
        let label = format_date(text, Locale::EnUs, DateStyle::Short).unwrap_or_else(|_| text.clone());
        let text_size = estimated_text_size(&label, font_size, None);
        // MyGo's `coreDateInput` is a button: `Padding(Space(1.5),
        // Space(3.5))` inside a 1 DIP border, its `MinWidth(Space(32.5))`
        // spreading the text and the icon apart.
        let icon_size = space * 3.5;
        let min_width = space * 32.5;
        let vertical = space * 1.5;
        let horizontal = space * 3.5;
        let inner_width = (text_size.width + icon_size + space * 3.0).max(min_width - horizontal * 2.0 - 2.0);
        let width = inner_width + horizontal * 2.0 + 2.0;
        let height = text_size.height.max(icon_size) + vertical * 2.0 + 2.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(root, id.to_string());
        self.register_focusable(root, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        let clicked = self.clicked_in_previous(id);
        let focused = self.focused_id.as_deref() == Some(id);
        let activated = clicked || (focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space)));
        if pressed || activated {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let hover_progress = self.animate(&format!("{id}:hover"), if hovered { 1.0 } else { 0.0 });
        let face = mix_color(theme.surface, theme.surface_hover, hover_progress.clamp(0.0, 1.0));
        let border = if self.shows_focus_ring(focused) {
            theme.accent
        } else {
            theme.border
        };
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(6.0),
                color: border,
            },
            DrawCommand::RoundedRect {
                x: 1.0,
                y: 1.0,
                width: (width - 2.0).max(1.0),
                height: (height - 2.0).max(1.0),
                radius: Radius::all(5.0),
                color: face,
            },
            DrawCommand::Text {
                x: horizontal + 1.0,
                y: vertical + 1.0,
                text: label.clone(),
                font_size,
                weight: 400,
                italic: false,
                mono: false,
                family: None,
                color: theme.foreground,
                max_width: Some(inner_width),
                alignment: TextAlignment::Start,
            },
        ];
        // The calendar icon: a page rounded 2, stroked 1.2 inside (1, 2),
        // with a 1.2-high rule 5 down it, as MyGo's `coreDateInput` draws
        // with `Stroke` and `Fill` (`ui/date.go`).
        let icon_x = width - horizontal - 1.0 - icon_size;
        // MyGo's button centers its children in the content box, which
        // starts past the border and the padding.
        let icon_y = 1.0 + (height - 2.0 - icon_size) / 2.0;
        let page_width = (icon_size - 2.0).max(1.0);
        let page_height = (icon_size - 3.0).max(1.0);
        // MyGo's `Painter.Stroke` fills its rectangle's border inward, a
        // whole device pixel wide (`ui/paint.go` borders: round(1.2*2) at
        // scale 2), so the ring is two filled rects one DIP apart, its
        // inside the field's own surface.
        commands.push(DrawCommand::RoundedRect {
            x: icon_x + 1.0,
            y: icon_y + 2.0,
            width: page_width,
            height: page_height,
            radius: Radius::all(2.0),
            color: theme.muted,
        });
        commands.push(DrawCommand::RoundedRect {
            x: icon_x + 2.0,
            y: icon_y + 3.0,
            width: (page_width - 2.0).max(1.0),
            height: (page_height - 2.0).max(1.0),
            radius: Radius::all(1.0),
            color: theme.surface,
        });
        commands.push(DrawCommand::Rect {
            x: icon_x + 1.0,
            y: icon_y + 5.0,
            width: page_width,
            height: 1.2,
            color: theme.muted,
        });
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.nodes.get_mut(&root).expect("date node").commands = commands;
        self.register_access_node(root, Role::Button, &label, None, focused);
        if activated {
            state.open = !state.open;
        }
        let parsed = parse_iso_date(text);
        let mut changed = false;
        let mut submitted = false;
        if let Some((year, month, day)) = parsed {
            if state.calendar.year != year
                || state.calendar.month != month
                || state.calendar.selected != Some(day)
            {
                state.calendar.year = year;
                state.calendar.month = month;
                state.calendar.selected = Some(day);
                changed = true;
            }
        }
        let anchor = self
            .previous_regions
            .get(id)
            .map(|region| region.bounds);
        let popup_id = format!("{id}:popup");
        let mut calendar_changed = false;
        if state.open {
            if let Some(anchor) = anchor {
                let mut overlay_state = OverlayState::new(true);
                let mut calendar_response = None;
                let response = self.overlay(&popup_id, &mut overlay_state, anchor, |context| {
                    calendar_response =
                        Some(context.calendar(&format!("{id}:calendar"), &mut state.calendar));
                });
                state.open = overlay_state.open && response.open;
                if let Some(calendar) = calendar_response {
                    if calendar.changed {
                        if let Some(day) = calendar.selected {
                            *text =
                                format!("{:04}-{:02}-{:02}", calendar.year, calendar.month, day);
                            changed = true;
                            calendar_changed = true;
                        }
                    }
                    if calendar.clicked.is_some() {
                        state.open = false;
                        self.close_overlay_runtime(&popup_id);
                        self.focused_id = Some(id.to_string());
                        submitted = true;
                    }
                }
                if !state.open {
                    self.focused_id = Some(id.to_string());
                }
            }
        }
        let valid = parse_iso_date(text).is_some();
        DateInputResponse {
            open: state.open,
            valid,
            changed: changed || calendar_changed,
            submitted,
            focused,
        }
    }

    /// Creates a clickable text link.
    pub fn link(&mut self, id: &str, label: &str) -> LinkState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = measure_text_width(label, font_size);
        let height = measured_text_height(font_size);
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node, id.to_string());
        self.register_focusable(node, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let clicked = self.clicked_in_previous(id) || (focused && self.key_pressed(Key::Enter));
        let mut commands = vec![DrawCommand::Text {
            x: 0.0,
            y: 0.0,
            text: label.to_string(),
            font_size,
            italic: false,
            mono: false,
            family: None,
            weight: 400,
            color: if hovered || focused {
                theme.foreground
            } else {
                theme.accent
            },
            max_width: Some(width),
            alignment: TextAlignment::Start,
        }];
        if hovered || focused {
            commands.push(DrawCommand::Line {
                x1: 0.0,
                y1: font_size + 3.0,
                x2: width,
                y2: font_size + 3.0,
                width: 1.0,
                color: theme.accent,
            });
        }
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.register_access_node(node, Role::Button, label, Some("link"), focused);
        self.nodes
            .get_mut(&node)
            .expect("link node exists")
            .commands = commands;
        LinkState {
            clicked,
            focused,
            hovered,
        }
    }

    /// Draws a built-in SVG icon.
    pub fn icon(&mut self, id: &str, name: &str, size: f32, color: Color) -> IconState {
        let size = if size.is_finite() && size > 0.0 {
            size
        } else {
            0.0
        };
        let node = self.new_named_node(
            id,
            Style {
                width: Some(size),
                height: Some(size),
                ..Style::default()
            },
        );
        let icon = Icon::builtin(name);
        let commands = icon
            .as_ref()
            .filter(|_| size > 0.0)
            .map(|icon| icon.to_commands_with_direction(size, color, self.direction))
            .unwrap_or_default();
        let command_count = commands.len();
        self.nodes
            .get_mut(&node)
            .expect("icon node exists")
            .commands = commands;
        self.register_access_node(
            node,
            Role::Label,
            name,
            Some(if icon.is_some() { "icon" } else { "missing" }),
            false,
        );
        IconState {
            name: name.to_string(),
            size,
            found: icon.is_some(),
            commands: command_count,
        }
    }

    /// Draws an initials or image avatar.
    pub fn avatar(
        &mut self,
        id: &str,
        label: &str,
        initials: Option<&str>,
        data: Option<ImageData<'_>>,
        size: f32,
        shape: AvatarShape,
    ) -> AvatarState {
        // MyGo's Avatar (`ui/indicators.go` coreAvatar): FontSize * 2.25
        // across, radius half of it, clipped and centered. An image covers
        // it; otherwise the fill is the hue of the name's FNV-1a hash at 45%
        // saturation and 55% lightness, and the initials of its first and
        // last words sit at 600 weight and 0.4 of the size in white.
        let theme = self.theme.clone();
        // MyGo's avatar is `FontSize * 2.25` across (`ui/indicators.go`
        // coreAvatar); a size of zero asks for that, as this demo does.
        let size = if size.is_finite() && size > 0.0 {
            size
        } else {
            theme.font_sizes.body * 2.25
        };
        let node = self.new_named_node(
            id,
            Style {
                width: Some(size),
                height: Some(size),
                ..Style::default()
            },
        );
        let mut commands = Vec::new();
        let mut drawn = false;
        if size > 0.0 {
            let fill = if data.is_some() {
                None
            } else {
                Some(hsl_color(fnv1a_hue(label), 0.45, 0.55))
            };
            if let Some(color) = fill {
                // MyGo's avatar is a box rounded by half its size, which
                // `Painter.snap` rounds out to whole device pixels; a canvas
                // arc would leave half-covered rim pixels the source lacks.
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: size,
                    height: size,
                    radius: Radius::all(size / 2.0),
                    color,
                });
            }
            if let Some(data) = data {
                let (columns, rows) = scaled_rgba_commands(
                    data,
                    &mut commands,
                    size,
                    size,
                    shape == AvatarShape::Circle,
                    self.pixel_scale,
                );
                drawn = columns > 0 && rows > 0;
            }
            // MyGo returns an image avatar as soon as it builds the image
            // (`ui/indicators.go` coreAvatar), so initials show only over
            // the colored fill.
            if data.is_none() {
                // MyGo derives the initials from the name itself, so a caller
                // need not supply them; an explicit pair still wins.
                let text = initials
                    .map(str::to_string)
                    .unwrap_or_else(|| avatar_initials(label));
                let font_size = size * 0.4;
                // The box centers the text across its width; the alignment
                // offset is applied once, when the command is serialized.
                commands.push(DrawCommand::Text {
                    x: 0.0,
                    y: (size - measured_text_height(font_size)) / 2.0,
                    text,
                    font_size,
                    italic: false,
                    mono: false,
                    family: None,
                    weight: 600,
                    color: Color::WHITE,
                    max_width: Some(size.max(1.0)),
                    alignment: TextAlignment::Center,
                });
            }
        }
        self.nodes
            .get_mut(&node)
            .expect("avatar node exists")
            .commands = commands;
        self.register_access_node(
            node,
            Role::Label,
            label,
            Some(if data.is_some() {
                "image"
            } else {
                initials.unwrap_or("")
            }),
            false,
        );
        AvatarState {
            size,
            shape,
            has_image: data.is_some(),
            drawn,
        }
    }

    /// Creates a three-tone quantity meter whose thresholds name their
    /// sides, as MyGo's `Meter(c, v, lo, hi, &MeterLevels{Warning, Critical})`
    /// does: Warning and Critical are values of the same range, and a
    /// Critical below Warning makes low values the bad ones, as a battery's
    /// charge is. Both `None` shows the success color throughout.
    pub fn meter_levels(
        &mut self,
        id: &str,
        value: f32,
        min: f32,
        max: f32,
        warning: Option<f32>,
        critical: Option<f32>,
    ) -> MeterState {
        let theme = self.theme.clone();
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        let value = if value.is_finite() {
            value.clamp(min, max)
        } else {
            min
        };
        let clamp = |threshold: Option<f32>| {
            threshold.filter(|t| t.is_finite()).map(|t| t.clamp(min, max))
        };
        let (warning, critical) = (clamp(warning), clamp(critical));
        let fraction = if max > min {
            ((value - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // MyGo's levels (`ui/indicators.go` coreMeter): with Critical below
        // Warning, a value at or below Critical is the danger and one at or
        // below Warning the warning; otherwise the same the other way up.
        // Equal thresholds name no levels at all.
        let tone = match (warning, critical) {
            (Some(warning), Some(critical)) if warning != critical => {
                let low = critical < warning;
                if (!low && value >= critical) || (low && value <= critical) {
                    MeterTone::High
                } else if (!low && value >= warning) || (low && value <= warning) {
                    MeterTone::Medium
                } else {
                    MeterTone::Low
                }
            }
            _ => MeterTone::Low,
        };
        let width = self.current_content_width();
        // `Height(Space(2))` with `Radius(Space(1))`, its 20-Space minimum
        // width left to the caller's column.
        let height = 8.0;
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        let tone_color = match tone {
            MeterTone::Low => theme.success(),
            MeterTone::Medium => theme.warning(),
            MeterTone::High => theme.danger(),
        };
        let mut commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
            radius: Radius::all(4.0),
            color: theme.border,
        }];
        if fraction > 0.0 {
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: (width * fraction).max(2.0),
                height,
                radius: Radius::all(4.0),
                color: tone_color,
            });
        }
        self.nodes
            .get_mut(&node)
            .expect("meter node exists")
            .commands = commands;
        self.register_access_node(
            node,
            Role::ProgressBar,
            "meter",
            Some(format!("{:.2}/{}", value, max).as_str()),
            false,
        );
        MeterState {
            value,
            fraction,
            tone,
            min,
            max,
        }
    }

    /// Creates a three-tone quantity meter.
    pub fn meter(
        &mut self,
        id: &str,
        value: f32,
        min: f32,
        max: f32,
        low_threshold: f32,
        high_threshold: f32,
    ) -> MeterState {
        let theme = self.theme.clone();
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        let value = if value.is_finite() {
            value.clamp(min, max)
        } else {
            min
        };
        let low = if low_threshold.is_finite() {
            low_threshold.clamp(min, max)
        } else {
            min
        };
        let high = if high_threshold.is_finite() {
            high_threshold.clamp(min, max)
        } else {
            max
        };
        let (low, high) = if low <= high {
            (low, high)
        } else {
            (high, low)
        };
        let fraction = if max > min {
            ((value - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // MyGo's levels (`ui/indicators.go` coreMeter): the meter is green
        // at Success until the value passes Warning, and Danger past
        // Critical, counting down instead when Critical is the lower bound,
        // as a battery's charge is.
        let tone = if value < low {
            MeterTone::Low
        } else if value < high {
            MeterTone::Medium
        } else {
            MeterTone::High
        };
        let width = self.current_content_width();
        // `Height(Space(2))` with `Radius(Space(1))`, its 20-Space minimum
        // width left to the caller's column.
        let height = 8.0;
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        // The levels are passed lowest first, so a value at or past the
        // high threshold is the danger and one past the low is the warning;
        // anything below stays the success color.
        let tone_color = match tone {
            MeterTone::Low => theme.success(),
            MeterTone::Medium => theme.warning(),
            MeterTone::High => theme.danger(),
        };
        let mut commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
            radius: Radius::all(4.0),
            color: theme.border,
        }];
        if fraction > 0.0 {
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: (width * fraction).max(2.0),
                height,
                radius: Radius::all(4.0),
                color: tone_color,
            });
        }
        self.nodes
            .get_mut(&node)
            .expect("meter node exists")
            .commands = commands;
        self.register_access_node(
            node,
            Role::ProgressBar,
            "meter",
            Some(format!("{:.2}/{}", value, max).as_str()),
            false,
        );
        MeterState {
            value,
            fraction,
            tone,
            min,
            max,
        }
    }

    /// Creates a dual-knob range slider.
    pub fn range_slider(
        &mut self,
        id: &str,
        state: &mut RangeSliderState,
        min: f32,
        max: f32,
    ) -> RangeSliderResponse {
        let theme = self.theme.clone();
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        if !state.low.is_finite() {
            state.low = min;
        }
        if !state.high.is_finite() {
            state.high = max;
        }
        state.low = state.low.clamp(min, max);
        state.high = state.high.clamp(min, max);
        if state.low > state.high {
            std::mem::swap(&mut state.low, &mut state.high);
        }
        let previous_low = state.low;
        let previous_high = state.high;
        let width = self.current_content_width();
        // As MyGo's RangeSlider: a 20-high track band with two 20×16
        // capsules, each half inside the padding (`ui/indicators.go`).
        let height = 20.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(root, id.to_string());
        self.register_focusable(root, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let mut dragging = match self.widget_states.get(id) {
            Some(WidgetState::RangeSlider { dragging }) => *dragging,
            _ => None,
        };
        if pressed && dragging.is_none() {
            if let (Some(pointer), Some(region)) = (
                self.local_pointer_in_previous(id),
                self.previous_regions.get(id),
            ) {
                let fraction =
                    ((pointer.x - region.bounds.x) / region.bounds.width.max(1.0)).clamp(0.0, 1.0);
                let value = min + (max - min) * fraction;
                let to_low = (value - state.low).abs();
                let to_high = (value - state.high).abs();
                let knob = if to_low <= to_high {
                    RangeKnob::Low
                } else {
                    RangeKnob::High
                };
                state.active = knob;
                dragging = Some(knob);
                self.focused_id = Some(id.to_string());
            }
        }
        if let Some(knob) = dragging {
            if let (Some(pointer), Some(region)) = (
                self.local_pointer_in_previous(id),
                self.previous_regions.get(id),
            ) {
                let fraction =
                    ((pointer.x - region.bounds.x) / region.bounds.width.max(1.0)).clamp(0.0, 1.0);
                let value = min + (max - min) * fraction;
                match knob {
                    RangeKnob::Low => state.low = value.clamp(min, state.high),
                    RangeKnob::High => state.high = value.clamp(state.low, max),
                }
                state.active = knob;
            }
        }
        if !self.input.pointer_down {
            dragging = None;
        }
        if focused {
            let step = ((max - min) / 100.0).max(0.01);
            let delta = if self.key_pressed(Key::ArrowRight) || self.key_pressed(Key::ArrowUp) {
                Some(step)
            } else if self.key_pressed(Key::ArrowLeft) || self.key_pressed(Key::ArrowDown) {
                Some(-step)
            } else {
                None
            };
            if let Some(delta) = delta {
                match state.active {
                    RangeKnob::Low => state.low = (state.low + delta).clamp(min, state.high),
                    RangeKnob::High => state.high = (state.high + delta).clamp(state.low, max),
                }
            }
        }
        let changed = (state.low - previous_low).abs() > f32::EPSILON
            || (state.high - previous_high).abs() > f32::EPSILON;
        self.widget_states
            .insert(id.to_string(), WidgetState::RangeSlider { dragging });
        let low_fraction = if max > min {
            ((state.low - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let high_fraction = if max > min {
            ((state.high - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let low_x = low_fraction * (width - 20.0) + 10.0;
        let high_x = high_fraction * (width - 20.0) + 10.0;
        let cy = 10.0;
        let track_y = cy - 3.0;
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: track_y,
                width,
                height: 6.0,
                radius: Radius::all(3.0),
                color: theme.border,
            },
            DrawCommand::RoundedRect {
                x: low_x,
                y: track_y,
                width: (high_x - low_x).max(0.0),
                height: 6.0,
                radius: Radius::all(3.0),
                color: theme.accent,
            },
        ];
        // White capsules lifted by the tight edge and soft shadows MyGo's
        // paintKnob casts, as the single slider's knob.
        for x in [low_x, high_x] {
            commands.push(DrawCommand::Shadow {
                x: x - 10.0,
                y: cy - 8.0,
                width: 20.0,
                height: 16.0,
                radius: 8.0,
                offset_x: 0.0,
                offset_y: 0.5,
                blur: 1.0,
                color: Color::rgba(0, 0, 0, 20),
            });
            commands.push(DrawCommand::Shadow {
                x: x - 10.0,
                y: cy - 8.0,
                width: 20.0,
                height: 16.0,
                radius: 8.0,
                offset_x: 0.0,
                offset_y: 1.5,
                blur: 7.0,
                color: Color::rgba(0, 0, 0, 26),
            });
            commands.push(DrawCommand::RoundedRect {
                x: x - 10.0,
                y: cy - 8.0,
                width: 20.0,
                height: 16.0,
                radius: Radius::all(8.0),
                color: Color::rgb(255, 255, 255),
            });
        }
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.register_access_node(
            root,
            Role::Slider,
            "range slider",
            Some(format!("{:.2}..{:.2}", state.low, state.high).as_str()),
            focused,
        );
        self.nodes
            .get_mut(&root)
            .expect("range slider root exists")
            .commands = commands;
        RangeSliderResponse {
            low: state.low,
            high: state.high,
            changed,
            dragging,
            active: state.active,
            focused,
            hovered,
        }
    }

    /// Creates a star rating from zero through `max`.
    pub fn rating(&mut self, id: &str, value: &mut u32, max: u32) -> RatingState {
        let theme = self.theme.clone();
        *value = (*value).min(max);
        // As MyGo: stars a body font size and a quarter big, two apart
        // (`ui/indicators.go` coreRating).
        let star_size = theme.font_sizes.body * 1.25;
        let gap = 2.0;
        let width = star_size * max as f32 + gap * (max.saturating_sub(1)) as f32;
        let height = star_size;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                gap,
                ..Style::default()
            },
        );
        self.register_focusable(root, id.to_string());
        let previous = *value;
        let mut clicked = None;
        for index in 0..max {
            let key = format!("{id}:star:{index}");
            if self.clicked_in_previous(&key) {
                clicked = Some(index + 1);
                self.focused_id = Some(id.to_string());
                break;
            }
        }
        if let Some(next) = clicked {
            *value = next.min(max);
        }
        let focused = self.focused_id.as_deref() == Some(id);
        if focused {
            let next = if self.key_pressed(Key::ArrowRight) || self.key_pressed(Key::ArrowUp) {
                Some((*value + 1).min(max))
            } else if self.key_pressed(Key::ArrowLeft) || self.key_pressed(Key::ArrowDown) {
                Some(value.saturating_sub(1))
            } else if self.key_pressed(Key::Home) {
                Some(0)
            } else if self.key_pressed(Key::End) {
                Some(max)
            } else {
                None
            };
            if let Some(next) = next {
                *value = next;
            }
        }
        let changed = *value != previous;
        let hover_preview = (0..max).find_map(|index| {
            let key = format!("{id}:star:{index}");
            self.pointer_in_previous(&key).then_some(index + 1)
        });
        let filled = hover_preview.unwrap_or(*value);
        // MyGo's stars glow in the theme's Warning, dimmed while the pointer
        // picks a count (`ui/indicators.go` coreRating).
        let warning = theme.warning();
        let mut root_commands = Vec::new();
        if self.shows_focus_ring(focused) {
            root_commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.nodes
            .get_mut(&root)
            .expect("rating root exists")
            .commands = root_commands;
        self.push_parent(root);
        for index in 0..max {
            let key = format!("{id}:star:{index}");
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(star_size),
                    height: Some(star_size),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            let hovered = self.pointer_in_previous(&key);
            if hovered && self.input.pointer_down {
                self.focused_id = Some(id.to_string());
            }
            let selected = index < filled;
            let star_color = if hover_preview.is_some() {
                Color::rgba(warning.r, warning.g, warning.b, 153)
            } else if selected {
                warning
            } else {
                theme.muted
            };
            // MyGo draws a star as a path of the node's own box
            // (`ui/indicators.go` starPath), not as an icon scaled from a
            // view box: the outer points reach 0.48 of the width, the inner
            // 0.2, around a center a twenty-fifth of the height low, and the
            // path keeps the box's fractional position as FillPath does.
            let commands = vec![DrawCommand::SvgPath {
                x: 0.0,
                y: 0.0,
                width: star_size,
                height: star_size,
                path: star_path(star_size),
                fill: selected.then_some(star_color),
                stroke: (!selected).then_some(star_color),
                // MyGo strokes the star's outline 1.2 DIPs wide
                // (`ui/indicators.go` coreRating).
                stroke_width: 1.2,
            }];
            self.register_access_node(
                node,
                Role::Button,
                &format!("star {}", index + 1),
                Some(if selected { "filled" } else { "empty" }),
                false,
            );
            self.nodes
                .get_mut(&node)
                .expect("rating star exists")
                .commands = commands;
        }
        self.pop_parent();
        self.register_access_node(
            root,
            Role::Slider,
            "rating",
            Some(format!("{}/{}", *value, max).as_str()),
            focused,
        );
        RatingState {
            changed,
            value: *value,
            focused,
            hover_preview,
        }
    }

    /// Creates an integer stepper with minus/plus controls.
    pub fn stepper(
        &mut self,
        id: &str,
        value: &mut i64,
        min: i64,
        max: i64,
        step: i64,
    ) -> StepperState {
        let theme = self.theme.clone();
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        let step = step.max(1);
        *value = (*value).clamp(min, max);
        // MyGo's stepper is a compact two-arrow column: 20 wide, two
        // 14-high arrow boxes over a 1px border and surface fill
        // (`ui/indicators.go` coreStepper).
        let width = 20.0;
        let row_height = 14.0;
        let height = 30.0;
        let radius = 6.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                padding: Edges::all(1.0),
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
        );
        if let Some(node) = self.nodes.get_mut(&root) {
            node.fill = Some((theme.surface, Radius::all(radius)));
            node.stroke = Some((theme.border, Radius::all(radius)));
        }
        self.register_interaction(root, id.to_string());
        self.register_focusable(root, id.to_string());
        let hovered = self.pointer_in_previous(id);
        if hovered && self.input.pointer_down {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let decrease_key = format!("{id}:dec");
        let increase_key = format!("{id}:inc");
        let previous = *value;
        if self.clicked_in_previous(&decrease_key) || (focused && self.key_pressed(Key::ArrowDown))
        {
            *value = (*value).saturating_sub(step).clamp(min, max);
        }
        if self.clicked_in_previous(&increase_key) || (focused && self.key_pressed(Key::ArrowUp)) {
            *value = (*value).saturating_add(step).clamp(min, max);
        }
        if self.clicked_in_previous(&decrease_key) || self.clicked_in_previous(&increase_key) {
            self.focused_id = Some(id.to_string());
        }
        let changed = *value != previous;
        self.push_parent(root);
        let box_width = width - 2.0;
        for (key, up) in [(increase_key.as_str(), true), (decrease_key.as_str(), false)] {
            let node = self.new_named_node(
                key,
                Style {
                    width: Some(box_width),
                    height: Some(row_height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.to_string());
            self.register_access_node(
                node,
                Role::Button,
                if up { "increase" } else { "decrease" },
                None,
                false,
            );
            let arrow_hovered = self.pointer_in_previous(key);
            let pressed = arrow_hovered && self.input.pointer_down;
            let can = if up { *value < max } else { *value > min };
            let mut commands = Vec::new();
            if pressed {
                commands.push(DrawCommand::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: box_width,
                    height: row_height,
                    color: theme.surface_pressed,
                });
            } else if arrow_hovered {
                commands.push(DrawCommand::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: box_width,
                    height: row_height,
                    color: theme.surface_hover,
                });
            }
            let cx = box_width / 2.0;
            let cy = row_height / 2.0;
            let d = row_height * 0.22;
            let path = if up {
                format!(
                    "M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}",
                    cx - d,
                    cy + d / 2.0,
                    cx,
                    cy - d / 2.0,
                    cx + d,
                    cy + d / 2.0
                )
            } else {
                format!(
                    "M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}",
                    cx - d,
                    cy - d / 2.0,
                    cx,
                    cy + d / 2.0,
                    cx + d,
                    cy - d / 2.0
                )
            };
            let color = if can {
                theme.foreground
            } else {
                Color::rgba(theme.muted.r, theme.muted.g, theme.muted.b, 128)
            };
            commands.push(DrawCommand::SvgPath {
                x: 0.0,
                y: 0.0,
                width: d * 2.0,
                height: d,
                path,
                fill: None,
                stroke: Some(color),
                stroke_width: 1.5,
            });
            self.nodes
                .get_mut(&node)
                .expect("stepper arrow exists")
                .commands = commands;
        }
        self.pop_parent();
        self.register_access_node(
            root,
            Role::Slider,
            "stepper",
            Some(value.to_string().as_str()),
            focused,
        );
        StepperState {
            changed,
            value: *value,
            focused,
            hovered,
        }
    }

    /// Creates a numeric text field with filtering and arrow stepping.
    pub fn number_input(
        &mut self,
        id: &str,
        text: &mut String,
        options: NumberInputOptions,
    ) -> NumberInputState {
        let decimals = options.decimals.min(18);
        let step = if options.step.is_finite() && options.step != 0.0 {
            options.step.abs()
        } else {
            1.0
        };
        let allow_negative = options.allow_negative;
        let focused = self.focused_id.as_deref() == Some(id);
        let mut stepped = false;
        if focused {
            let direction = if self.key_pressed(Key::ArrowUp) {
                Some(step)
            } else if self.key_pressed(Key::ArrowDown) {
                Some(-step)
            } else {
                None
            };
            if let Some(delta) = direction {
                let current = text.parse::<f64>().unwrap_or(0.0);
                let mut next = current + delta;
                if next.is_finite() {
                    if !allow_negative && next < 0.0 {
                        next = 0.0;
                    }
                    *text = format!("{next:.prec$}", prec = decimals);
                    stepped = true;
                }
            }
        }
        let filter = move |raw: &str| filter_numeric_text(raw, allow_negative, decimals);
        let field = self.text_field_impl(id, text, Some(&filter), false, None, false);
        let value = if text.is_empty() {
            None
        } else {
            text.parse::<f64>().ok()
        };
        NumberInputState {
            changed: stepped || field.changed,
            value,
            focused: field.focused,
            hovered: field.hovered,
        }
    }

    /// Creates a fixed-point decimal text field.
    pub fn decimal_input(
        &mut self,
        id: &str,
        text: &mut String,
        options: DecimalInputOptions,
    ) -> DecimalInputState {
        let decimals = options.decimals.min(18);
        let allow_negative = options.allow_negative;
        let filter = move |raw: &str| filter_numeric_text(raw, allow_negative, decimals);
        let field = self.text_field_impl(id, text, Some(&filter), false, None, false);
        let units = parse_decimal_units(text, decimals);
        DecimalInputState {
            changed: field.changed,
            units,
            valid: units.is_some(),
            focused: field.focused,
            hovered: field.hovered,
        }
    }

    /// Creates a checkbox that toggles by mouse or focused Enter/Space.
    pub fn checkbox(&mut self, id: &str, checked: &mut bool) -> CheckboxState {
        self.checkbox_impl(id, None, checked, false)
    }

    /// Creates a checkbox that shows a caller-supplied label next to the box.
    pub fn checkbox_labeled(
        &mut self,
        id: &str,
        label: &str,
        checked: &mut bool,
    ) -> CheckboxState {
        self.checkbox_impl(id, Some(label), checked, false)
    }

    /// Creates a checkbox rendered in the mixed/indeterminate state. A click
    /// resolves it to checked; the caller should stop passing `indeterminate`
    /// on the next frame.
    pub fn checkbox_indeterminate(
        &mut self,
        id: &str,
        label: &str,
        checked: &mut bool,
        indeterminate: bool,
    ) -> CheckboxState {
        self.checkbox_impl(id, Some(label), checked, indeterminate)
    }

    fn checkbox_impl(
        &mut self,
        id: &str,
        label: Option<&str>,
        checked: &mut bool,
        indeterminate: bool,
    ) -> CheckboxState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        if !matches!(self.widget_states.get(id), Some(WidgetState::Checkbox)) {
            self.widget_states
                .insert(id.to_string(), WidgetState::Checkbox);
        }
        // MyGo's check box row is as tall as its line of text: the 16-pixel
        // box fits in the 17-pixel line.
        let height = measured_text_height(font_size).max(16.0);
        let box_size = 16.0;
        let label_width = label
            .map(|text| measure_text_width(text, font_size))
            .unwrap_or(0.0);
        let width = if label.is_some() {
            box_size + 8.0 + label_width
        } else {
            box_size
        };
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.register_focusable(node_id, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let mouse_clicked = self.clicked_in_previous(id);
        let keyboard_clicked =
            focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space));
        let changed = mouse_clicked || keyboard_clicked;
        if changed {
            if indeterminate {
                *checked = true;
            } else {
                *checked = !*checked;
            }
        }
        let mut commands = Vec::new();
        // MyGo centers the box in the row, which is a pixel taller than it
        // is: half a pixel down, where its edges fall on whole device
        // pixels at a scale of two (`ui/feedback.go` paintCheckbox).
        let box_top = (height - box_size) / 2.0;
        if *checked || indeterminate {
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: box_top,
                width: box_size,
                height: box_size,
                radius: Radius::all(4.0),
                color: theme.accent,
            });
        } else {
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: box_top,
                width: box_size,
                height: box_size,
                radius: Radius::all(4.0),
                color: mix_color(theme.border, theme.foreground, 0.25),
            });
            commands.push(DrawCommand::RoundedRect {
                x: 1.0,
                y: box_top + 1.0,
                width: box_size - 2.0,
                height: box_size - 2.0,
                radius: Radius::all(3.0),
                color: theme.background,
            });
        }
        if let Some(label) = label {
            commands.push(DrawCommand::Text {
                x: 24.0,
                y: 0.0,
                text: label.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: 400,
                color: theme.foreground,
                max_width: None,
                alignment: TextAlignment::Start,
            });
        }
        if indeterminate {
            commands.push(DrawCommand::RoundedRect {
                x: box_size * 0.25,
                y: box_top + box_size / 2.0 - 1.0,
                width: box_size * 0.5,
                height: 2.0,
                radius: Radius::all(1.0),
                color: theme.on_accent,
            });
        } else if *checked {
            // MyGo's check path, through the box (`checkPath`).
            commands.push(DrawCommand::Line {
                x1: box_size * 0.22,
                y1: box_top + box_size * 0.52,
                x2: box_size * 0.42,
                y2: box_top + box_size * 0.71,
                width: 2.0,
                color: theme.on_accent,
            });
            commands.push(DrawCommand::Line {
                x1: box_size * 0.42,
                y1: box_top + box_size * 0.71,
                x2: box_size * 0.78,
                y2: box_top + box_size * 0.31,
                width: 2.0,
                color: theme.on_accent,
            });
        }
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.register_access_node(
            node_id,
            Role::CheckBox,
            "checkbox",
            Some(if indeterminate {
                "mixed"
            } else if *checked {
                "checked"
            } else {
                "unchecked"
            }),
            focused,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("checkbox node exists")
            .commands = commands;
        CheckboxState {
            changed,
            checked: *checked,
            focused,
            hovered,
        }
    }

    /// Creates a horizontal slider controlled by drag or focused arrow keys.
    pub fn slider(&mut self, id: &str, value: &mut f32, min: f32, max: f32) -> SliderState {
        // As MyGo, a slider is the stepped one without ticks: one track
        // and knob, 20 high.
        self.slider_impl(id, value, min, max, 0.0)
    }

    /// Creates a stepped slider with tick marks, as MyGo's StepSlider.
    ///
    /// Values snap to `min` plus multiples of `step` up to `max`, and arrow
    /// keys move by exactly one step. A ticked slider is taller to make room
    /// for the marks below its track (`ui/widgets.go` slider).
    pub fn slider_stepped(
        &mut self,
        id: &str,
        value: &mut f32,
        min: f32,
        max: f32,
        step: f32,
    ) -> SliderState {
        self.slider_impl(id, value, min, max, step)
    }

    fn slider_impl(
        &mut self,
        id: &str,
        value: &mut f32,
        min: f32,
        max: f32,
        step: f32,
    ) -> SliderState {
        let theme = self.theme.clone();
        let min = min.min(max);
        let max = max.max(min);
        let step = if step.is_finite() { step.max(0.0) } else { 0.0 };
        *value = value.clamp(min, max);
        if step > 0.0 {
            *value = snap_slider_value(*value, min, max, step);
        }
        if !matches!(self.widget_states.get(id), Some(WidgetState::Slider { .. })) {
            self.widget_states
                .insert(id.to_string(), WidgetState::Slider { dragging: false });
        }
        let tick_count = slider_tick_count(min, max, step);
        let height = if tick_count > 0 { 28.0 } else { 20.0 };
        let width = self.current_content_width();
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.register_focusable(node_id, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pointer_pressed = hovered && self.input.pointer_down;
        if pointer_pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let keyboard_right = focused && self.key_pressed(Key::ArrowRight);
        let keyboard_left = focused && self.key_pressed(Key::ArrowLeft);
        let pointer_down = self.input.pointer_down;
        let local_pointer = self.local_pointer_in_previous(id);
        let (dragging, changed) = match self.widget_states.get_mut(id) {
            Some(WidgetState::Slider { dragging }) => {
                if pointer_pressed {
                    *dragging = true;
                }
                let mut changed = false;
                if *dragging {
                    if let (Some(pointer), Some(region)) =
                        (local_pointer, self.previous_regions.get(id))
                    {
                        let fraction = ((pointer.x - region.bounds.x)
                            / region.bounds.width.max(1.0))
                        .clamp(0.0, 1.0);
                        let next = snap_slider_value(min + (max - min) * fraction, min, max, step);
                        if (*value - next).abs() > f32::EPSILON {
                            *value = next;
                            changed = true;
                        }
                    }
                }
                if !pointer_down {
                    *dragging = false;
                }
                let keyboard_step = if step > 0.0 { step } else { (max - min) / 100.0 };
                if keyboard_right {
                    *value = (*value + keyboard_step).clamp(min, max);
                    changed = true;
                }
                if keyboard_left {
                    *value = (*value - keyboard_step).clamp(min, max);
                    changed = true;
                }
                (*dragging, changed)
            }
            _ => (false, false),
        };
        let fraction = if max > min {
            ((*value - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let knob_x = fraction * (width - 20.0) + 10.0;
        // The track sits Spaces(2.5) from the top and spans the whole node,
        // its knob half inside the padding; ticks hang below it. MyGo keeps
        // that top band fixed when a stepped slider grows for its ticks.
        let cy = 10.0;
        let track_y = cy - 3.0;
        let mut commands = Vec::new();
        if tick_count > 0 {
            let tick_color =
                Color::rgba(theme.muted.r, theme.muted.g, theme.muted.b, 153);
            let n = tick_count as f32;
            for i in 0..tick_count {
                let x = 10.0 + (width - 20.0) * i as f32 / (n - 1.0).max(1.0);
                commands.push(DrawCommand::RoundedRect {
                    x: x - 0.5,
                    y: 22.0,
                    width: 1.0,
                    height: 6.0,
                    radius: Radius::all(0.0),
                    color: tick_color,
                });
            }
        }
        commands.extend_from_slice(&[
            DrawCommand::RoundedRect {
                x: 0.0,
                y: track_y,
                width: width.max(1.0),
                height: 6.0,
                radius: Radius::all(3.0),
                color: theme.border,
            },
            DrawCommand::RoundedRect {
                x: 0.0,
                y: track_y,
                width: knob_x.max(1.0),
                height: 6.0,
                radius: Radius::all(3.0),
                color: theme.accent,
            },
            // A white capsule edged by a dark rim, lifted by the tight edge
            // and soft shadows MyGo's paintKnob casts with it.
            DrawCommand::Shadow {
                x: knob_x - 10.0,
                y: cy - 8.0,
                width: 20.0,
                height: 16.0,
                radius: 8.0,
                offset_x: 0.0,
                offset_y: 0.5,
                blur: 1.0,
                color: Color::rgba(0, 0, 0, 20),
            },
            DrawCommand::Shadow {
                x: knob_x - 10.0,
                y: cy - 8.0,
                width: 20.0,
                height: 16.0,
                radius: 8.0,
                offset_x: 0.0,
                offset_y: 1.5,
                blur: 7.0,
                color: Color::rgba(0, 0, 0, 26),
            },
            // MyGo's knob at rest is a plain white capsule: no rim, only
            // the shadows above lift it off the track.
            DrawCommand::RoundedRect {
                x: knob_x - 10.0,
                y: cy - 8.0,
                width: 20.0,
                height: 16.0,
                radius: Radius::all(8.0),
                color: Color::rgb(255, 255, 255),
            },
        ]);
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        let value_text = format!("{value:.2}");
        self.register_access_node(
            node_id,
            Role::Slider,
            "slider",
            Some(value_text.as_str()),
            focused,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("slider node exists")
            .commands = commands;
        SliderState {
            changed,
            dragging,
            focused,
            hovered,
        }
    }

    /// Creates a vertically scrollable clipped region.
    ///
    /// Wheel input and scrollbar dragging update the persistent offset. There
    /// is no inertia, animated scrollbar, or nested scroll coordination.
    pub fn scroll_view(
        &mut self,
        id: &str,
        viewport_height: f32,
        content: impl FnOnce(&mut Context),
    ) -> ScrollState {
        self.scroll_view_styled(id, viewport_height, ScrollStyle::default(), content)
    }

    /// Creates a scroll view with a face and padding, as MyGo's Scroll takes
    /// Background, Border, Radius and Padding.
    pub fn scroll_view_styled(
        &mut self,
        id: &str,
        viewport_height: f32,
        style: ScrollStyle,
        content: impl FnOnce(&mut Context),
    ) -> ScrollState {
        let theme = self.theme.clone();
        let viewport_height = viewport_height.max(1.0);
        let width = self.current_content_width();
        // MyGo's border takes layout room (`ui/layout.go` padX = pad +
        // border), so the content sits one pixel further in than the fill's
        // own padding would put it.
        let edge = if style.border.is_some() { 1.0 } else { 0.0 };
        let padding = Edges::new(
            style.padding.top + edge,
            style.padding.right + edge,
            style.padding.bottom + edge,
            style.padding.left + edge,
        );
        if !matches!(
            self.widget_states.get(id),
            Some(WidgetState::ScrollView { .. })
        ) {
            self.widget_states.insert(
                id.to_string(),
                WidgetState::ScrollView {
                    offset: 0.0,
                    dragging: false,
                },
            );
        }
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(viewport_height),
                flex_direction: FlexDirection::Column,
                gap: style.gap,
                padding,
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.push_parent(node_id);
        content(self);
        self.pop_parent();
        let inner_width = (width - padding.horizontal()).max(1.0);
        // MyGo's content height is the content box plus both paddings and
        // borders (`ui/layout.go` contentH = max(uh, ch) + padY, padY being
        // padding and border); the layout places the children inside the top
        // padding, so only the bottom edge is left to add.
        let content_height = self.estimated_content_height(node_id, inner_width) + padding.bottom;
        let max_scroll = (content_height - viewport_height).max(0.0);
        // A view that starts at its end, as MyGo's log does with its
        // `TrackScroll` state at `math.MaxFloat32`.
        if let Some(initial) = style.initial_offset {
            if let Some(WidgetState::ScrollView { offset, .. }) = self.widget_states.get_mut(id) {
                if *offset == 0.0 {
                    *offset = if initial.is_finite() { initial } else { max_scroll }
                        .clamp(0.0, max_scroll);
                }
            }
        }
        let hovered = self.pointer_in_previous(id);
        let local_pointer = self.local_pointer_in_previous(id);
        let mut changed = false;
        let (offset, dragging) = match self.widget_states.get_mut(id) {
            Some(WidgetState::ScrollView { offset, dragging }) => {
                if hovered && self.input.wheel.1.abs() > f32::EPSILON {
                    *offset = (*offset + self.input.wheel.1).clamp(0.0, max_scroll);
                    changed = true;
                }
                if hovered && self.input.pointer_down {
                    if let (Some(pointer), Some(region)) =
                        (local_pointer, self.previous_regions.get(id))
                    {
                        if pointer.x >= region.bounds.right() - 10.0 {
                            *dragging = true;
                        }
                    }
                }
                if *dragging {
                    if let (Some(pointer), Some(region)) =
                        (local_pointer, self.previous_regions.get(id))
                    {
                        let fraction = ((pointer.y - region.bounds.y)
                            / region.bounds.height.max(1.0))
                        .clamp(0.0, 1.0);
                        *offset = fraction * max_scroll;
                        changed = true;
                    }
                }
                if !self.input.pointer_down {
                    *dragging = false;
                }
                (offset.clamp(0.0, max_scroll), *dragging)
            }
            _ => (0.0, false),
        };
        let thumb_height = if max_scroll > 0.0 {
            (viewport_height * viewport_height / content_height.max(viewport_height))
                .clamp(18.0, viewport_height)
        } else {
            viewport_height
        };
        let thumb_y = if max_scroll > 0.0 {
            (offset / max_scroll) * (viewport_height - thumb_height)
        } else {
            0.0
        };
        let _ = (theme, thumb_height, thumb_y);
        let node = self.nodes.get_mut(&node_id).expect("scroll node exists");
        // MyGo's Scroll draws a face when it has one, its border outside the
        // face as a control's is; the browser's scrollbar is an overlay that
        // stays out of the picture until the pointer moves.
        let mut commands = Vec::new();
        if let Some(color) = style.background {
            if let Some(border) = style.border {
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height: viewport_height,
                    radius: Radius::all(style.radius),
                    color: border,
                });
                commands.push(DrawCommand::RoundedRect {
                    x: 1.0,
                    y: 1.0,
                    width: (width - 2.0).max(0.0),
                    height: (viewport_height - 2.0).max(0.0),
                    radius: Radius::all((style.radius - 1.0).max(0.0)),
                    color,
                });
            } else {
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height: viewport_height,
                    radius: Radius::all(style.radius),
                    color,
                });
            }
        }
        node.commands = commands;
        node.overlay_commands = Vec::new();
        node.scroll = Some(ScrollNode {
            offset,
            viewport_height,
        });
        self.register_access_node(node_id, Role::ScrollView, "scroll view", None, false);
        ScrollState {
            scroll: offset,
            max_scroll,
            changed,
            dragging,
            hovered,
        }
    }

    /// Creates a flexible spacer.
    pub fn spacer(&mut self) {
        self.new_node(Style {
            flex_grow: 1.0,
            ..Style::default()
        });
    }

    /// Returns the current layout scope.
    pub fn current_scope(&self) -> &LayoutScope {
        &self.scope
    }

    pub(crate) fn begin_frame(&mut self, size: Size) {
        self.available_size = normalize_size(size);
        let now = self.frame_time_override.unwrap_or_else(Instant::now);
        self.frame_time = now.checked_add(self.clock_offset).unwrap_or(now);
        self.frame_serial = self.frame_serial.saturating_add(1);
        let current_frame = self.frame_serial;
        self.animations
            .retain(|_, animation| animation.last_seen_frame.saturating_add(1) >= current_frame);
        self.previous_regions = std::mem::take(&mut self.current_regions);
        self.current_regions.clear();
        self.previous_interaction_transforms =
            std::mem::take(&mut self.current_interaction_transforms);
        self.current_interaction_transforms.clear();
        self.current_transform = Transform::identity();
        self.previous_overlays = std::mem::take(&mut self.current_overlays);
        self.nodes.clear();
        self.roots.clear();
        self.parent_stack.clear();
        self.explicit_id = None;
        self.pending_interactions.clear();
        self.pending_focusables.clear();
        self.current_focus_key = None;
        self.pending_access_nodes.clear();
        self.next_id = 1;
        self.current_layer = 0;
        self.next_overlay_layer = 0;
        self.frame_overlays.clear();
        self.pending_focus_layer = None;
        self.requested_animation = false;
    }

    pub(crate) fn set_input(&mut self, event: UiEvent) {
        match event {
            UiEvent::PointerMoved { x, y } => {
                self.input.pointer = Some(Point::new(x, y));
            }
            UiEvent::PointerPressed { x, y, button } => {
                self.input.pointer = Some(Point::new(x, y));
                // A press focuses what it hits without drawing a ring, as
                // MyGo's input clears `focusVisible` (`ui/input.go`).
                self.focus_visible = false;
                if button == crate::event::PointerButton::Left {
                    self.input.pointer_down = true;
                    self.input.released = None;
                } else {
                    self.input.secondary_pressed = Some(Point::new(x, y));
                }
            }
            UiEvent::PointerReleased { x, y, button } => {
                self.input.pointer = Some(Point::new(x, y));
                if button == crate::event::PointerButton::Left {
                    self.input.pointer_down = false;
                    self.input.released = Some(Point::new(x, y));
                }
            }
            UiEvent::Wheel { delta_x, delta_y } => {
                self.input.wheel.0 += delta_x;
                self.input.wheel.1 += delta_y;
            }
            UiEvent::TextInput { text } => {
                self.input.text.push_str(&text);
            }
            UiEvent::Key { key } => {
                if key == Key::Tab {
                    self.cycle_focus(false);
                    self.focus_visible = true;
                } else if key == Key::ShiftTab {
                    self.cycle_focus(true);
                    self.focus_visible = true;
                }
                self.input.keys.push(key);
            }
        }
    }

    pub(crate) fn end_frame(&mut self) {
        self.input.released = None;
        self.input.secondary_pressed = None;
        self.input.wheel = (0.0, 0.0);
        self.input.text.clear();
        self.input.keys.clear();
    }

    #[cfg(test)]
    pub(crate) fn set_reduce_motion_for_test(&mut self, reduce_motion: bool) {
        self.reduce_motion = reduce_motion;
    }

    #[cfg(test)]
    pub(crate) fn advance_frame_time_for_test(&mut self, duration: Duration) {
        self.clock_offset = self.clock_offset.saturating_add(duration);
        self.frame_time = self
            .frame_time
            .checked_add(duration)
            .unwrap_or(self.frame_time);
    }

    #[cfg(test)]
    pub(crate) fn set_frame_time_for_test(&mut self, time: Instant) {
        self.frame_time_override = Some(time);
        self.clock_offset = Duration::ZERO;
        self.frame_time = time;
    }

    pub(crate) fn needs_animation_frame(&self) -> bool {
        self.requested_animation
            || self
                .animations
                .values()
                .any(|animation| animation.active && animation.last_seen_frame == self.frame_serial)
    }

    pub(crate) fn produce(&mut self) -> Result<super::native::ViewFrame, String> {
        let total_start = Instant::now();
        let layout_start = Instant::now();
        let layout = self.compute_layout();
        let layout_elapsed = layout_start.elapsed();
        let paint_start = Instant::now();
        let mut display = DisplayList::new();
        let mut ordered_roots = self.roots.clone();
        ordered_roots.sort_by_key(|id| self.nodes.get(id).map(|node| node.layer).unwrap_or(0));
        for root in &ordered_roots {
            self.append_display(*root, &layout, &mut display);
        }
        let mut interactions = resolve_button_regions(&layout, &self.pending_interactions);
        interactions.sort_by_key(|region| {
            self.nodes
                .get(&region.node_id)
                .map(|node| node.layer)
                .unwrap_or(0)
        });
        self.current_regions = interactions
            .iter()
            .map(|region| (region.button_id.clone(), region.clone()))
            .collect();
        self.current_layers.clear();
        for region in &interactions {
            if let Some(node) = self.nodes.get(&region.node_id) {
                self.current_layers
                    .insert(region.button_id.clone(), node.layer);
            }
        }
        self.current_overlays.clear();
        for (node_id, overlay_id, layer) in &self.frame_overlays {
            if let Some(result) = layout.iter().find(|result| result.id == *node_id) {
                self.current_overlays.insert(
                    overlay_id.clone(),
                    OverlayFrame {
                        layer: *layer,
                        bounds: Rect::new(result.x, result.y, result.width, result.height),
                    },
                );
            }
        }
        let top_layer = self.frame_overlays.iter().map(|(_, _, layer)| *layer).max();
        self.focus_order = self
            .pending_focusables
            .iter()
            .filter(|(node_id, _, layer)| {
                layout.iter().any(|result| result.id == *node_id)
                    && top_layer.is_none_or(|top| *layer == top)
            })
            .map(|(_, key, _)| key.clone())
            .collect();
        if let Some(layer) = self.pending_focus_layer.take() {
            let first = self
                .pending_focusables
                .iter()
                .find(|(node_id, _, focus_layer)| {
                    *focus_layer == layer && layout.iter().any(|result| result.id == *node_id)
                })
                .map(|(_, key, _)| key.clone());
            if first.is_some() {
                self.focused_id = first;
            }
        }
        let mut counts = CommandCounts::default();
        for command in &display.commands {
            counts.record(command);
        }
        let stats = RenderStats {
            total_commands: display.len(),
            command_counts: counts,
            unsupported_commands: counts.unsupported(),
            effective_bounds: display.bounds(),
            ..RenderStats::default()
        };
        let access_nodes = self
            .pending_access_nodes
            .iter()
            .filter_map(|pending| {
                let result = layout.iter().find(|result| result.id == pending.node_id)?;
                Some(AccessNode {
                    role: pending.role,
                    label: pending.label.clone(),
                    value: pending.value.clone(),
                    disabled: false,
                    focused: pending.focused,
                    bounds: Rect::new(result.x, result.y, result.width, result.height),
                })
            })
            .collect::<Vec<_>>();
        let paint_elapsed = paint_start.elapsed();
        let total_elapsed = total_start.elapsed();
        self.last_frame_stats =
            FrameStats::from_durations(layout_elapsed, paint_elapsed, total_elapsed);
        Ok(super::native::ViewFrame {
            layout,
            display,
            interactions,
            stats,
            access_nodes,
            needs_animation_frame: self.needs_animation_frame(),
            input_consumed: false,
        })
    }

    fn compute_layout(&self) -> Vec<LayoutResult> {
        if self.roots.is_empty() {
            return Vec::new();
        }
        let single_root_is_flow = self.roots.len() == 1
            && self
                .nodes
                .get(&self.roots[0])
                .map(|node| node.style.position == Position::Relative)
                .unwrap_or(false);
        let root = if single_root_is_flow {
            self.build_node(self.roots[0]).expect("root node exists")
        } else {
            let mut root = Node::new(u64::MAX).with_style(Style {
                width: Some(self.available_size.width),
                height: Some(self.available_size.height),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                ..Style::default()
            });
            for child in &self.roots {
                if let Some(node) = self.build_node(*child) {
                    root.children.push(node);
                }
            }
            root
        };
        LayoutEngine::compute(&root, self.available_size)
    }

    fn build_node(&self, id: u64) -> Option<Node> {
        let pending = self.nodes.get(&id)?;
        let mut node = Node::new(id)
            .with_style(pending.style)
            .with_content_size(pending.content_size);
        if let Some(baseline) = pending.first_baseline {
            node = node.with_first_baseline(baseline);
        }
        if let Some((ascent, descent)) = pending.text_metrics {
            node = node.with_text_metrics(ascent, descent);
        }
        for child in &pending.children {
            if let Some(child_node) = self.build_node(*child) {
                node.children.push(child_node);
            }
        }
        if pending.rtl && pending.style.flex_direction == FlexDirection::Row {
            let mut relative = Vec::new();
            let mut absolute = Vec::new();
            for child in node.children {
                if child.style.position == Position::Relative {
                    relative.push(child);
                } else {
                    absolute.push(child);
                }
            }
            relative.reverse();
            node.children = relative;
            node.children.extend(absolute);
        }
        Some(node)
    }

    fn append_display(&self, id: u64, layout: &[LayoutResult], display: &mut DisplayList) {
        let Some(pending) = self.nodes.get(&id) else {
            return;
        };
        let result = layout.iter().find(|result| result.id == id).copied();
        if let Some(result) = result {
            let mut painted = Vec::new();
            if let Some((color, radius)) = pending.fill {
                if color.a == 0 {
                    // A transparent face with a border is a stroke only; an
                    // opaque inner face would hide the node's surroundings.
                    if let Some((border, border_radius)) = pending.stroke {
                        painted.push(stroked_rounded_rect(
                            result.width,
                            result.height,
                            border_radius.top_left,
                            border,
                        ));
                    }
                } else if color.a < 255 {
                    // A translucent face over a border: paint the edge, then
                    // the face over whatever is behind it, as MyGo's
                    // Background(alpha).Border(1) does.
                    if let Some((border, border_radius)) = pending.stroke {
                        painted.push(stroked_rounded_rect(
                            result.width,
                            result.height,
                            border_radius.top_left,
                            border,
                        ));
                    }
                    painted.push(DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: result.width,
                        height: result.height,
                        radius,
                        color,
                    });
                } else if let Some((border, border_radius)) = pending.stroke {
                    painted.push(DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: result.width,
                        height: result.height,
                        radius: border_radius,
                        color: border,
                    });
                    if result.width > 2.0 && result.height > 2.0 {
                        painted.push(DrawCommand::RoundedRect {
                            x: 1.0,
                            y: 1.0,
                            width: result.width - 2.0,
                            height: result.height - 2.0,
                            radius: Radius::all((radius.top_left - 1.0).max(0.0)),
                            color,
                        });
                    }
                } else {
                    painted.push(DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: result.width,
                        height: result.height,
                        radius,
                        color,
                    });
                }
            } else if let Some((border, radius)) = pending.stroke {
                painted.push(stroked_rounded_rect(
                    result.width,
                    result.height,
                    radius.top_left,
                    border,
                ));
            }
            if let Some(color) = pending.top_border {
                painted.push(DrawCommand::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: result.width,
                    height: 1.0,
                    color,
                });
            }
            if let Some(color) = pending.bottom_border {
                painted.push(DrawCommand::Rect {
                    x: 0.0,
                    y: result.height - 1.0,
                    width: result.width,
                    height: 1.0,
                    color,
                });
            }
            painted.extend(pending.commands.clone());
            let mut local = DisplayList::from_commands(painted);
            if pending.transform != Transform::identity() {
                local.commands.insert(
                    0,
                    DrawCommand::TransformPush {
                        transform: pending.transform,
                    },
                );
                local.commands.push(DrawCommand::TransformPop);
            }
            local.translate(Point::new(result.x, result.y));
            display.extend(local.commands);
            if let Some(scroll) = &pending.scroll {
                display.push(DrawCommand::ScissorPush {
                    rect: Rect::new(
                        result.x,
                        result.y,
                        result.width,
                        scroll.viewport_height.min(result.height),
                    ),
                });
                display.push(DrawCommand::TransformPush {
                    transform: Transform::translate(Point::new(0.0, -scroll.offset)),
                });
            } else if pending.clip {
                // A clip is the node's own box: only a Scroll insets its
                // content inside the border it paints (`ui/paint.go`).
                display.push(DrawCommand::ScissorPush {
                    rect: Rect::new(result.x, result.y, result.width, result.height),
                });
            }
        }
        for child in &pending.children {
            self.append_display(*child, layout, display);
        }
        if pending.scroll.is_some() {
            display.push(DrawCommand::TransformPop);
            display.push(DrawCommand::ScissorPop);
        } else if pending.clip && result.is_some() {
            display.push(DrawCommand::ScissorPop);
        }
        if let Some(result) = result {
            let mut overlay = DisplayList::from_commands(pending.overlay_commands.clone());
            overlay.translate(Point::new(result.x, result.y));
            display.extend(overlay.commands);
        }
    }

    /// Returns the width the children of `node_id` ask for, so that a
    /// panel sizes to the column it holds rather than to its trigger.
    fn estimated_content_width(&self, node_id: u64) -> f32 {
        let Some(node) = self.build_node(node_id) else {
            return 0.0;
        };
        // What the content asks for, as the layout's own intrinsic size
        // measures it: a column with a width of its own reports that, and
        // one without reports the widest row it holds.
        super::layout::intrinsic_size(&node).width
    }

    fn estimated_content_height(&self, node_id: u64, width: f32) -> f32 {
        let Some(node) = self.build_node(node_id) else {
            return 0.0;
        };
        LayoutEngine::compute(&node, Size::new(width.max(1.0), 1.0))
            .into_iter()
            .filter(|result| result.id != node_id)
            .map(|result| result.y + result.height)
            .fold(0.0_f32, f32::max)
    }

    fn new_named_node(&mut self, key: &str, style: Style) -> u64 {
        self.explicit_id = Some(key.to_string());
        self.new_node(style)
    }

    fn widget_state_text_field(&mut self, id: &str) {
        if !matches!(
            self.widget_states.get(id),
            Some(WidgetState::TextField { .. })
        ) {
            self.widget_states.insert(
                id.to_string(),
                WidgetState::TextField {
                    cursor: 0,
                    anchor: None,
                    cursor_x: 0.0,
                    cursor_y: 0.0,
                },
            );
        }
    }

    pub(crate) fn new_node(&mut self, style: Style) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let key = self.explicit_id.take();
        let pending = PendingNode {
            style,
            content_size: Size::default(),
            children: Vec::new(),
            commands: Vec::new(),
            overlay_commands: Vec::new(),
            scroll: None,
            key,
            layer: self.current_layer,
            transform: self.current_transform,
            rtl: self.direction.is_rtl(),
            fill: None,
            stroke: None,
            clip: false,
            top_border: None,
            bottom_border: None,
            first_baseline: None,
            text_metrics: None,
        };
        self.nodes.insert(id, pending);
        if let Some(parent) = self.parent_stack.last().copied() {
            self.nodes
                .get_mut(&parent)
                .expect("parent node exists")
                .children
                .push(id);
        } else {
            self.roots.push(id);
        }
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn update_text(
        &mut self,
        id: u64,
        text: &str,
        font_size: f32,
        weight: u16,
        color: Color,
        max_width: Option<f32>,
        alignment: TextAlignment,
        offset_y: f32,
        single_line: bool,
        mono: bool,
        family: Option<&str>,
    ) {
        // MyGo's SingleLine keeps one line and ends what does not fit with
        // an ellipsis (`ui/element.go` SingleLine, Ellipsis).
        let mut truncated = false;
        let text = if single_line {
            match max_width {
                Some(width) if width > 0.0 => {
                    let cut = truncate_with_ellipsis(text, font_size, width);
                    truncated = cut != text;
                    cut
                }
                _ => text.to_string(),
            }
        } else {
            text.to_string()
        };
        let mut size = estimated_text_size_styled(
            &text,
            font_size,
            if single_line { None } else { max_width },
            mono,
            weight,
            family,
        );
        // A node measures its text by its whole, untruncated width, and a
        // line that does not fit takes the width it has
        // (`ui/layout.go` measureIntrinsic; `ui/layoutcache.go`
        // textWidths measures at width 0). Its glyphs then start at the
        // content origin rather than centering in the room left over.
        if truncated {
            if let Some(width) = max_width {
                size.width = width;
            }
        }
        // MyGo lines a form field's row up on the baseline of its first line
        // of text (`ui/form.go` alignBaselines): the line box's top plus the
        // font's ascent, which is where the renderer puts the baseline.
        let ascent = host_text_ascent(font_size, false, mono, weight >= 600)
            .or_else(|| default_font().map(|font| font.ascent(font_size)))
            .unwrap_or(font_size * 0.93);
        // A text node draws inside its own padding, as MyGo's text does: its
        // line box starts at the content origin (`ui/paint.go` textLayout
        // takes `e.contentX()`, `e.contentY()`).
        let (pad_left, pad_top) = self
            .nodes
            .get(&id)
            .map(|node| (node.style.padding.left, node.style.padding.top))
            .unwrap_or((0.0, 0.0));
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        node.content_size = size;
        node.first_baseline = Some(pad_top + offset_y + ascent);
        node.text_metrics = Some((
            ascent,
            host_text_descent(font_size, false, mono, weight >= 600)
                .or_else(|| default_font().map(|font| font.descent(font_size)))
                .unwrap_or(font_size * 0.25),
        ));
        node.commands = vec![DrawCommand::Text {
            x: pad_left,
            y: pad_top + offset_y,
            text,
            font_size,
            weight,
            italic: false,
            mono,
            family: family.map(str::to_string),
            color,
            max_width,
            // A cut line starts at the content origin, as MyGo's does in
            // the full width its node takes.
            alignment: if truncated {
                TextAlignment::Start
            } else {
                alignment
            },
        }];
    }

    /// Clips a node the caller created with [`Self::new_node`] to its box.
    pub(crate) fn set_node_clip(&mut self, id: u64, clip: bool) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.clip = clip;
        }
    }

    /// Fills a node the caller created with [`Self::new_node`].
    pub(crate) fn set_node_fill(&mut self, id: u64, color: Color, radius: f32) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.fill = Some((color, Radius::all(radius)));
        }
    }

    /// Replaces a node's commands, as the chart marks paint their paths.
    pub(crate) fn set_node_commands(&mut self, id: u64, commands: Vec<DrawCommand>) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.commands = commands;
        }
    }

    /// Paints a one-pixel border around a node the caller created with
    /// [`Self::new_node`], as [`FlexBuilder::border`] does.
    pub(crate) fn set_node_border(&mut self, id: u64, color: Color, radius: f32) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.stroke = Some((color, Radius::all(radius)));
        }
    }

    /// Paints a one-pixel rule along a node's top edge, as MyGo's
    /// `BorderWidth(1, 0, 0, 0)` draws a section's divider.
    pub(crate) fn set_node_bottom_border(&mut self, id: u64, color: Color) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.bottom_border = Some(color);
        }
    }

    pub(crate) fn set_node_top_border(&mut self, id: u64, color: Color) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.top_border = Some(color);
        }
    }

    /// Names a node the caller created with [`Self::new_node`].
    pub(crate) fn set_node_key(&mut self, id: u64, key: &str) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.key = Some(key.to_string());
        }
    }

    /// Sets a node's single line of text, as [`TextBuilder`] does.
    pub(crate) fn set_node_text(
        &mut self,
        id: u64,
        text: &str,
        font_size: f32,
        color: Color,
        max_width: Option<f32>,
        alignment: TextAlignment,
        offset_y: f32,
    ) {
        self.update_text(
            id,
            text,
            font_size,
            400,
            color,
            max_width,
            alignment,
            offset_y,
            false,
            false,
            None,
        );
    }

    pub(crate) fn push_parent(&mut self, id: u64) {
        self.parent_stack.push(id);
    }

    pub(crate) fn pop_parent(&mut self) {
        self.parent_stack.pop();
    }

    fn register_focusable(&mut self, node_id: u64, key: String) {
        self.pending_focusables
            .push((node_id, key.clone(), self.current_layer));
        self.current_focus_key = Some(key);
    }

    fn register_interaction(&mut self, node_id: u64, key: String) {
        self.current_interaction_transforms
            .insert(key.clone(), self.current_transform);
        self.pending_interactions.push((node_id, key));
    }

    fn register_access_node(
        &mut self,
        node_id: u64,
        role: Role,
        label: &str,
        value: Option<&str>,
        focused: bool,
    ) {
        self.pending_access_nodes.push(PendingAccessNode {
            node_id,
            role,
            label: label.to_string(),
            value: value.map(str::to_string),
            focused,
        });
    }

    fn node_key(&self, node_id: u64) -> String {
        self.nodes
            .get(&node_id)
            .and_then(|node| node.key.clone())
            .unwrap_or_else(|| format!("widget-{node_id}"))
    }

    pub(crate) fn current_content_width(&self) -> f32 {
        // Walk out to the nearest ancestor whose width is known: a widget's
        // content width is that ancestor's width less the padding between
        // them, as a nested column's content is its parent's.
        let mut extra_padding = 0.0;
        for id in self.parent_stack.iter().rev() {
            let Some(node) = self.nodes.get(id) else {
                continue;
            };
            if let Some(width) = node.style.width {
                return (width - extra_padding - node.style.padding.horizontal()).max(1.0);
            }
            extra_padding += node.style.padding.horizontal();
        }
        (self.available_size.width - extra_padding).max(1.0)
    }

    pub(crate) fn current_content_height(&self) -> f32 {
        let parent = self.parent_stack.last().and_then(|id| self.nodes.get(id));
        let height = parent
            .and_then(|node| node.style.height)
            .unwrap_or(self.available_size.height);
        let padding = parent
            .map(|node| node.style.padding.vertical())
            .unwrap_or(0.0);
        (height - padding).max(1.0)
    }

    fn cycle_focus(&mut self, reverse: bool) {
        if self.focus_order.is_empty() {
            return;
        }
        let current = self
            .focused_id
            .as_ref()
            .and_then(|focused| self.focus_order.iter().position(|id| id == focused));
        let next = match (current, reverse) {
            (Some(index), false) => (index + 1) % self.focus_order.len(),
            (Some(0), true) | (None, true) => self.focus_order.len() - 1,
            (Some(index), true) => index - 1,
            (None, false) => 0,
        };
        self.focused_id = Some(self.focus_order[next].clone());
    }

    fn key_pressed(&self, key: Key) -> bool {
        !self.widgets_disabled && self.input.keys.contains(&key)
    }

    /// Whether a focused widget draws its focus ring: MyGo shows one only
    /// while the keyboard moved the focus (`ui/input.go` FocusVisible).
    fn shows_focus_ring(&self, focused: bool) -> bool {
        focused && self.focus_visible
    }

    /// Whether a widget on `layer` may observe pointer input this frame.
    ///
    /// The topmost overlay captures the pointer for its layer; everything
    /// below it is inert, including the click that dismisses the overlay.
    fn pointer_available(&self, layer: u32) -> bool {
        self.previous_overlays
            .values()
            .map(|overlay| overlay.layer)
            .max()
            .is_none_or(|top| layer >= top)
    }

    fn pointer_in_previous(&self, id: &str) -> bool {
        if self.widgets_disabled {
            return false;
        }
        if !self.pointer_available(self.current_layer) {
            return false;
        }
        self.input
            .pointer
            .and_then(|pointer| {
                self.previous_regions.get(id).and_then(|region| {
                    self.local_pointer_for(id, pointer)
                        .map(|local| region.contains(local.x, local.y))
                })
            })
            .unwrap_or(false)
    }

    fn clicked_in_previous(&self, id: &str) -> bool {
        if self.widgets_disabled {
            return false;
        }
        if !self.pointer_available(self.current_layer) {
            return false;
        }
        self.input
            .released
            .and_then(|point| {
                self.previous_regions.get(id).and_then(|region| {
                    self.local_pointer_for(id, point)
                        .map(|local| region.contains(local.x, local.y))
                })
            })
            .unwrap_or(false)
    }

    fn local_pointer_for(&self, id: &str, point: Point) -> Option<Point> {
        let region = self.previous_regions.get(id)?;
        let transform = self
            .previous_interaction_transforms
            .get(id)
            .copied()
            .unwrap_or_else(Transform::identity);
        Some(inverse_transform_point(transform, point, region.bounds))
    }

    fn local_pointer_in_previous(&self, id: &str) -> Option<Point> {
        let point = self.input.pointer?;
        self.local_pointer_for(id, point)
    }

    /// Builds an anchored popup above the rest of the view.
    ///
    /// The caller owns `state.open`. Focus moves to the first focusable inside
    /// the popup while it is open, Tab traversal is confined to the top
    /// overlay layer, and closing restores the previously focused widget.
    pub fn overlay(
        &mut self,
        id: &str,
        state: &mut OverlayState,
        anchor: Rect,
        content: impl FnOnce(&mut Context),
    ) -> OverlayResponse {
        let response = self.overlay_popup(id, state.open, Some(anchor), content);
        state.open = response.open;
        response
    }

    fn overlay_popup(
        &mut self,
        id: &str,
        open: bool,
        anchor: Option<Rect>,
        content: impl FnOnce(&mut Context),
    ) -> OverlayResponse {
        let anchor = anchor.unwrap_or_else(|| {
            let width = self.available_size().width.min(280.0);
            Rect::new(
                (self.available_size().width - width) / 2.0,
                (self.available_size().height - 120.0) / 2.0,
                width,
                32.0,
            )
        });
        let mut runtime = self.overlay_runtimes.remove(id).unwrap_or_default();
        if !open {
            let closed = runtime.was_open;
            runtime.was_open = false;
            if closed {
                if let Some(focus) = runtime.restore_focus.take() {
                    self.focused_id = Some(focus);
                }
            }
            self.overlay_runtimes.insert(id.to_string(), runtime);
            return OverlayResponse {
                open: false,
                closed,
                ..OverlayResponse::default()
            };
        }
        let opening = !runtime.was_open;
        if opening {
            runtime.restore_focus = self.focused_id.clone();
            self.focused_id = None;
        }
        let previous_bounds = self.previous_overlays.get(id).map(|overlay| overlay.bounds);
        let outside_clicked = runtime.was_open
            && self
                .input
                .released
                .zip(previous_bounds)
                .map(|(point, bounds)| !rect_contains(bounds, point))
                .unwrap_or(false);
        let escape_pressed = self.key_pressed(Key::Escape);
        if outside_clicked || escape_pressed {
            runtime.was_open = false;
            if let Some(focus) = runtime.restore_focus.take() {
                self.focused_id = Some(focus);
            }
            self.overlay_runtimes.insert(id.to_string(), runtime);
            return OverlayResponse {
                open: false,
                closed: true,
                outside_clicked,
                escape_pressed,
                bounds: previous_bounds,
            };
        }

        let theme = self.theme.clone();
        let layer = self.allocate_overlay_layer();
        let previous_layer = self.current_layer;
        self.current_layer = layer;
        // MyGo's `stylePanel`: Space(1) of padding inside a one-pixel
        // border, which takes layout space there and does not here
        // (`ui/widgets.go`). The panel is as wide as its content asks and
        // no narrower than its trigger, as `MinWidth(anchor.Bounds().W)`
        // lets it grow.
        let inset = theme.spacing.base + 1.0;
        let gap = theme.spacing.xs;
        let saved_parents = std::mem::take(&mut self.parent_stack);
        self.explicit_id = Some(id.to_string());
        let node_id = self.new_node(Style {
            position: Position::Absolute,
            flex_direction: FlexDirection::Column,
            padding: Edges::all(inset),
            ..Style::default()
        });
        self.push_parent(node_id);
        content(self);
        self.pop_parent();
        self.parent_stack = saved_parents;
        self.current_layer = previous_layer;

        // `intrinsic_size` already counts the panel's own padding.
        let content_width = self.estimated_content_width(node_id);
        let width = content_width
            .max(anchor.width)
            .max(160.0)
            .min(self.available_size.width.max(160.0));
        // MyGo's `stylePanel` lifts the panel Space(1) off its trigger's
        // bottom edge (`ui/widgets.go`), which is where the 4px gap comes
        // from rather than the theme's own spacing.
        let x = anchor
            .x
            .clamp(0.0, (self.available_size.width - width).max(0.0));
        let mut y = anchor.y + anchor.height + theme.spacing.base;
        let max_height = (self.available_size.height - 8.0).max(36.0);
        let content_height = self.estimated_content_height(node_id, width) + inset;
        let height = content_height.max(36.0).min(max_height);
        if y + height > self.available_size.height && anchor.y - height - gap >= 0.0 {
            y = anchor.y - height - gap;
        }
        // A panel scrolls only when its content outgrows the window, as a
        // select's list of options does; one that fits is left whole
        // (`ui/widgets.go` stylePanel, `ui/base.go` popover).
        let scrolls = content_height > max_height;
        let node = self.nodes.get_mut(&node_id).expect("overlay node exists");
        node.style.width = Some(width);
        node.style.left = Some(x);
        node.style.top = Some(y);
        node.style.height = Some(height);
        node.scroll = scrolls.then_some(ScrollNode {
            offset: 0.0,
            viewport_height: height,
        });
        node.commands = if self.embedded_overlay {
            vec![DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(10.0),
                color: theme.surface,
            }]
        } else {
            vec![
                // MyGo's `stylePanel` shadow: 6 down, 20 of blur, 18% black
                // (`ui/widgets.go`).
                DrawCommand::Shadow {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                    radius: 8.0,
                    offset_x: 0.0,
                    offset_y: 6.0,
                    blur: 20.0,
                    color: Color::rgba(0, 0, 0, 46),
                },
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                    radius: Radius::all(8.0),
                    color: theme.border,
                },
                // MyGo's `stylePanel` fills the panel with the page's own
                // background, not the raised surface (`ui/widgets.go`).
                DrawCommand::RoundedRect {
                    x: 1.0,
                    y: 1.0,
                    width: (width - 2.0).max(1.0),
                    height: (height - 2.0).max(1.0),
                    radius: Radius::all(7.0),
                    color: theme.background,
                },
            ]
        };
        self.register_access_node(node_id, Role::Popup, id, None, false);
        self.frame_overlays.push((node_id, id.to_string(), layer));
        if opening {
            self.pending_focus_layer = Some(layer);
        }
        runtime.was_open = true;
        self.overlay_runtimes.insert(id.to_string(), runtime);
        OverlayResponse {
            open: true,
            closed: false,
            outside_clicked: false,
            escape_pressed: false,
            bounds: previous_bounds,
        }
    }

    fn allocate_overlay_layer(&mut self) -> u32 {
        self.next_overlay_layer = self.next_overlay_layer.max(1).saturating_add(1);
        self.next_overlay_layer
    }

    fn close_overlay_runtime(&mut self, id: &str) {
        if let Some(runtime) = self.overlay_runtimes.remove(id) {
            if runtime.was_open {
                if let Some(focus) = runtime.restore_focus {
                    self.focused_id = Some(focus);
                }
            }
        }
    }

    /// Creates an on/off switch controlled by click or focused Enter/Space.
    pub fn switch(&mut self, id: &str, checked: &mut bool) -> SwitchState {
        let theme = self.theme.clone();
        // MyGo's switch: 36 by 20, a 16-pixel white knob inset 2
        // (`ui/widgets.go` coreSwitch).
        let width = 36.0;
        let height = 20.0;
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.register_focusable(node_id, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let toggled = self.clicked_in_previous(id)
            || (focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space)));
        if toggled {
            *checked = !*checked;
        }
        let progress = self.animate(&format!("{id}:switch"), if *checked { 1.0 } else { 0.0 });
        let off = mix_color(theme.border, theme.foreground, 0.15);
        let track = if progress > 0.0 {
            mix_color(off, theme.accent, progress)
        } else {
            off
        };
        let knob_size = height - 4.0;
        let knob_x = 2.0 + progress * (width - height);
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: width,
                height: height,
                radius: Radius::all(height / 2.0),
                color: track,
            },
            // MyGo shadows the knob (0, 1, blur 3, 25% black).
            DrawCommand::Shadow {
                x: knob_x,
                y: 2.0,
                width: knob_size,
                height: knob_size,
                radius: knob_size / 2.0,
                offset_x: 0.0,
                offset_y: 1.0,
                blur: 3.0,
                color: Color::rgba(0, 0, 0, 64),
            },
            DrawCommand::Circle {
                x: knob_x + knob_size / 2.0,
                y: height / 2.0,
                radius: knob_size / 2.0,
                color: Color::rgb(255, 255, 255),
            },
        ];
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.register_access_node(
            node_id,
            Role::Switch,
            "switch",
            Some(if *checked { "on" } else { "off" }),
            focused,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("switch node exists")
            .commands = commands;
        SwitchState {
            changed: toggled,
            checked: *checked,
            focused,
            hovered,
        }
    }

    /// Creates a radio group controlled by mouse or focused arrow keys.
    pub fn radio_group(
        &mut self,
        id: &str,
        selected: &mut usize,
        options: &[&str],
    ) -> RadioGroupState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        let row_height = measured_text_height(font_size).max(20.0);
        let gap = theme.spacing.xs;
        if !options.is_empty() {
            *selected = (*selected).min(options.len() - 1);
        } else {
            *selected = 0;
        }
        let height = if options.is_empty() {
            row_height
        } else {
            options.len() as f32 * row_height + gap * (options.len() - 1) as f32
        };
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                flex_direction: FlexDirection::Column,
                gap,
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.register_focusable(node_id, id.to_string());
        let group_hovered = self.pointer_in_previous(id);
        if group_hovered && self.input.pointer_down {
            self.focused_id = Some(id.to_string());
        }
        self.push_parent(node_id);
        let mut hovered = group_hovered;
        let mut clicked_index = None;
        let mut pressed_index = None;
        for (index, option) in options.iter().enumerate() {
            let key = format!("{id}:{index}");
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(width),
                    height: Some(row_height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            let option_hovered = self.pointer_in_previous(&key);
            if option_hovered {
                hovered = true;
            }
            if option_hovered && self.input.pointer_down {
                pressed_index = Some(index);
            }
            if self.clicked_in_previous(&key) {
                clicked_index = Some(index);
            }
            let selected_option = index == *selected;
            let text_color = if selected_option {
                theme.foreground
            } else {
                theme.muted
            };
            let mut commands = vec![DrawCommand::Circle {
                x: 10.0,
                y: row_height / 2.0,
                radius: 8.0,
                color: if selected_option {
                    theme.accent
                } else {
                    theme.border
                },
            }];
            commands.push(DrawCommand::Circle {
                x: 10.0,
                y: row_height / 2.0,
                radius: if selected_option { 4.0 } else { 6.0 },
                color: if selected_option {
                    theme.on_accent
                } else {
                    theme.surface
                },
            });
            commands.push(DrawCommand::Text {
                x: 24.0,
                y: (row_height - measured_text_height(font_size)) / 2.0,
                text: option.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: 400,
                color: text_color,
                max_width: Some((width - 24.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
            self.register_access_node(
                node,
                Role::Radio,
                option,
                Some(if selected_option {
                    "selected"
                } else {
                    "unselected"
                }),
                false,
            );
            self.nodes
                .get_mut(&node)
                .expect("radio option exists")
                .commands = commands;
        }
        self.pop_parent();
        if pressed_index.or(clicked_index).is_some() {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let mut changed = false;
        if let Some(index) = clicked_index {
            if index != *selected {
                *selected = index;
                changed = true;
            }
        }
        if focused {
            let next = if self.key_pressed(Key::ArrowRight) || self.key_pressed(Key::ArrowDown) {
                Some((*selected + 1).min(options.len().saturating_sub(1)))
            } else if self.key_pressed(Key::ArrowLeft) || self.key_pressed(Key::ArrowUp) {
                Some(selected.saturating_sub(1))
            } else {
                None
            };
            if let Some(next) = next {
                if next != *selected {
                    *selected = next;
                    changed = true;
                }
            }
        }
        let mut commands = Vec::new();
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.register_access_node(node_id, Role::RadioGroup, id, None, focused);
        self.nodes
            .get_mut(&node_id)
            .expect("radio group node exists")
            .commands = commands;
        RadioGroupState {
            changed,
            selected: *selected,
            focused,
            hovered,
        }
    }

    /// Creates a drop-down select. The list is an overlay above the view.
    pub fn select(&mut self, id: &str, selected: &mut usize, options: &[&str]) -> SelectState {
        self.select_impl(id, selected, options, None)
    }

    /// Creates a select as wide as `width`, as MyGo's `Width` makes one
    /// fill a field's column (`ui/widgets.go` coreSelect).
    pub fn select_sized(
        &mut self,
        id: &str,
        selected: &mut usize,
        options: &[&str],
        width: f32,
    ) -> SelectState {
        self.select_impl(id, selected, options, Some(width))
    }

    fn select_impl(
        &mut self,
        id: &str,
        selected: &mut usize,
        options: &[&str],
        fixed_width: Option<f32>,
    ) -> SelectState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        // MyGo's select is a button 140 wide at least, its label left and
        // its chevron right (`ui/widgets.go` coreSelect).
        let padding_x = 15.0;
        let padding_y = 7.0;
        let label_width =
            measure_text_width(options.get(*selected).copied().unwrap_or(""), font_size);
        let width = fixed_width
            .unwrap_or((padding_x * 2.0 + label_width + 6.0 + 10.0).max(140.0));
        let height = measured_text_height(font_size) + padding_y * 2.0;
        if !options.is_empty() {
            *selected = (*selected).min(options.len() - 1);
        } else {
            *selected = 0;
        }
        let (mut open, mut highlight) = match self.widget_states.get(id) {
            Some(WidgetState::Select { open, highlight }) => (*open, *highlight),
            _ => (false, *selected),
        };
        if !options.is_empty() {
            highlight = highlight.min(options.len() - 1);
        } else {
            highlight = 0;
            open = false;
        }
        if !open {
            highlight = *selected;
        }
        let was_open = open;
        let bounds = self.previous_regions.get(id).map(|region| region.bounds);
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.register_focusable(node_id, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        if self.clicked_in_previous(id) {
            open = true;
            highlight = *selected;
        }
        if focused
            && !open
            && (self.key_pressed(Key::Enter)
                || self.key_pressed(Key::Space)
                || self.key_pressed(Key::ArrowDown))
        {
            open = true;
            highlight = *selected;
        }
        let mut chosen = None;
        if open && was_open {
            if self.key_pressed(Key::ArrowDown) && !options.is_empty() {
                highlight = (highlight + 1).min(options.len() - 1);
            }
            if self.key_pressed(Key::ArrowUp) {
                highlight = highlight.saturating_sub(1);
            }
            if self.key_pressed(Key::Enter) && !options.is_empty() {
                chosen = Some(highlight);
            }
        }
        let selected_label = options.get(*selected).copied().unwrap_or("");
        let face = if pressed {
            theme.surface_pressed
        } else if hovered {
            theme.surface_hover
        } else {
            theme.surface
        };
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.fill = Some((face, Radius::all(6.0)));
            node.stroke = Some((theme.border, Radius::all(6.0)));
        }
        let mut commands = vec![
            DrawCommand::Text {
                x: padding_x,
                y: (height - measured_text_height(font_size)) / 2.0,
                text: selected_label.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: 400,
                color: theme.foreground,
                max_width: Some((width - padding_x * 2.0 - 16.0).max(1.0)),
                alignment: TextAlignment::Start,
            },
        ];
        let chevron_x = width - padding_x - 10.0;
        let chevron_y = (height - 10.0) / 2.0;
        commands.push(DrawCommand::SvgPath {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            path: format!(
                "M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}",
                chevron_x + 1.0,
                chevron_y + 3.0,
                chevron_x + 5.0,
                chevron_y + 7.0,
                chevron_x + 9.0,
                chevron_y + 3.0
            ),
            fill: None,
            stroke: Some(theme.muted),
            stroke_width: 1.5,
        });
        if open || self.shows_focus_ring(focused) {
            commands.extend(focus_ring(
                0.0,
                0.0,
                width,
                height,
                if open { theme.accent } else { theme.border },
            ));
        }
        self.register_access_node(
            node_id,
            Role::ComboBox,
            "select",
            Some(selected_label),
            focused,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("select node exists")
            .commands = commands;
        // The label is the select's first line of text, so a form's row
        // lines the field's label up on its baseline (`ui/form.go` anchor).
        let select_ascent = host_text_ascent(font_size, false, false, false)
            .or_else(|| default_font().map(|font| font.ascent(font_size)))
            .unwrap_or(font_size * 0.93);
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.first_baseline = Some((height - measured_text_height(font_size)) / 2.0 + select_ascent);
        }

        if open {
            if let Some(anchor) = bounds {
                let popup_id = format!("{id}:popup");
                let highlight_value = highlight;
                let selected_value = *selected;
                let response = self.overlay_popup(&popup_id, true, Some(anchor), |context| {
                    for (index, option) in options.iter().enumerate() {
                        let key = format!("{id}:option:{index}");
                        let clicked = context.select_option(
                            &key,
                            option,
                            index == highlight_value,
                            index == selected_value,
                        );
                        if clicked {
                            chosen = Some(index);
                        }
                    }
                });
                if !response.open {
                    open = false;
                }
            }
        }
        let mut changed = false;
        if let Some(index) = chosen {
            if index != *selected {
                *selected = index;
                changed = true;
            }
            highlight = index;
            open = false;
            self.close_overlay_runtime(&format!("{id}:popup"));
        }
        self.widget_states
            .insert(id.to_string(), WidgetState::Select { open, highlight });
        SelectState {
            changed,
            open,
            selected: *selected,
            focused,
            hovered,
        }
    }

    fn select_option(&mut self, id: &str, label: &str, highlighted: bool, selected: bool) -> bool {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        let height = measured_text_height(font_size) + theme.spacing.xs * 2.0;
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let clicked = self.clicked_in_previous(id);
        let background = if highlighted {
            mix_color(theme.surface, theme.accent, 0.24)
        } else if hovered {
            mix_color(theme.surface, theme.foreground, 0.08)
        } else {
            theme.surface
        };
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(4.0),
                color: background,
            },
            DrawCommand::Text {
                x: theme.spacing.sm,
                y: theme.spacing.xs,
                text: label.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: if selected { 600 } else { 400 },
                color: if selected {
                    theme.accent
                } else {
                    theme.foreground
                },
                max_width: Some((width - theme.spacing.sm * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            },
        ];
        if selected {
            let y = height / 2.0;
            commands.push(DrawCommand::Line {
                x1: width - 16.0,
                y1: y,
                x2: width - 12.0,
                y2: y + 4.0,
                width: 2.0,
                color: theme.accent,
            });
            commands.push(DrawCommand::Line {
                x1: width - 12.0,
                y1: y + 4.0,
                x2: width - 5.0,
                y2: y - 4.0,
                width: 2.0,
                color: theme.accent,
            });
        }
        self.register_access_node(
            node_id,
            Role::Option,
            label,
            Some(if selected { "selected" } else { "unselected" }),
            false,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("select option node exists")
            .commands = commands;
        clicked
    }

    /// Creates a row of tabs choosing one selected index.
    pub fn tabs(&mut self, id: &str, selected: &mut usize, labels: &[&str]) -> TabsState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        if !labels.is_empty() {
            *selected = (*selected).min(labels.len() - 1);
        } else {
            *selected = 0;
        }
        let height = measured_text_height(font_size) + 16.0;
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                gap: 4.0,
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.register_focusable(node_id, id.to_string());
        self.push_parent(node_id);
        let mut hovered = false;
        let mut clicked_index = None;
        for (index, label) in labels.iter().enumerate() {
            let key = format!("{id}:tab:{index}");
            let selected_tab = index == *selected;
            // MyGo measures the chosen tab in the weight it shows it in,
            // `FontWeight(600)` (`ui/tabs.go` coreTabs).
            let label_width = host_text_width_weighted(
                label,
                font_size,
                false,
                false,
                if selected_tab { 600 } else { 400 },
            )
            .unwrap_or_else(|| measure_text_width(label, font_size));
            let tab_width = label_width + 24.0;
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(tab_width),
                    height: Some(height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            let tab_hovered = self.pointer_in_previous(&key);
            if tab_hovered {
                hovered = true;
            }
            if tab_hovered && self.input.pointer_down {
                self.focused_id = Some(id.to_string());
            }
            if self.clicked_in_previous(&key) {
                clicked_index = Some(index);
                self.focused_id = Some(id.to_string());
            }
            let color = if selected_tab {
                theme.foreground
            } else if tab_hovered {
                theme.foreground
            } else {
                theme.muted
            };
            let mut commands = vec![DrawCommand::Text {
                x: 12.0,
                y: 8.0,
                text: label.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: if selected_tab { 600 } else { 400 },
                color,
                max_width: Some((tab_width - 24.0).max(1.0)),
                alignment: TextAlignment::Start,
            }];
            if selected_tab {
                // MyGo draws the chosen tab's line inside the tab's padding.
                commands.push(DrawCommand::RoundedRect {
                    x: 6.0,
                    y: height - 2.0,
                    width: tab_width - 12.0,
                    height: 2.0,
                    radius: Radius::all(1.0),
                    color: theme.accent,
                });
            }
            self.register_access_node(
                node,
                Role::Tab,
                label,
                Some(if selected_tab {
                    "selected"
                } else {
                    "unselected"
                }),
                false,
            );
            self.nodes.get_mut(&node).expect("tab node exists").commands = commands;
        }
        self.pop_parent();
        let focused = self.focused_id.as_deref() == Some(id);
        let mut changed = false;
        if let Some(index) = clicked_index {
            if index != *selected {
                *selected = index;
                changed = true;
            }
        }
        if focused {
            let next = if self.key_pressed(Key::ArrowRight) {
                Some((*selected + 1).min(labels.len().saturating_sub(1)))
            } else if self.key_pressed(Key::ArrowLeft) {
                Some(selected.saturating_sub(1))
            } else {
                None
            };
            if let Some(next) = next {
                if next != *selected {
                    *selected = next;
                    changed = true;
                }
            }
        }
        let mut commands = Vec::new();
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.register_access_node(node_id, Role::TabList, id, None, focused);
        self.nodes
            .get_mut(&node_id)
            .expect("tabs node exists")
            .commands = commands;
        TabsState {
            changed,
            selected: *selected,
            focused,
            hovered,
        }
    }

    /// Creates a determinate progress bar.
    pub fn progress(&mut self, id: &str, value: f32, max: f32) -> ProgressState {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        // MyGo's progress bar is `Height(Space(1.5))` with `Space(0.75)`
        // corners (`ui/widgets.go` coreProgress).
        let height = theme.spacing.base * 1.5;
        let radius = theme.spacing.base * 0.75;
        let max = if max.is_finite() && max > 0.0 {
            max
        } else {
            1.0
        };
        let value = if value.is_finite() {
            value.clamp(0.0, max)
        } else {
            0.0
        };
        let fraction = (value / max).clamp(0.0, 1.0);
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        let mut commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
            radius: Radius::all(radius),
            color: theme.border,
        }];
        if fraction > 0.0 {
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: (width * fraction).max(2.0),
                height,
                radius: Radius::all(radius),
                color: theme.accent,
            });
        }
        self.register_access_node(
            node_id,
            Role::ProgressBar,
            "progress",
            Some(format!("{:.0}%", fraction * 100.0).as_str()),
            false,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("progress node exists")
            .commands = commands;
        ProgressState {
            value,
            fraction,
            indeterminate: false,
        }
    }

    /// Creates an indeterminate progress bar that requests animation frames.
    pub fn progress_indeterminate(&mut self, id: &str) -> ProgressState {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let height = theme.spacing.base * 1.5;
        let radius = theme.spacing.base * 0.75;
        if !self.reduce_motion {
            self.request_animation_frame();
        }
        // MyGo moves a segment 0.3 of the bar wide from a full width left of
        // it to a full width past it, on a 1400ms period, clipped to the bar
        // so it slides in and out (`ui/widgets.go` coreProgress).
        let phase = if self.reduce_motion {
            0.0
        } else {
            (self.frame_time.elapsed().as_secs_f32() * 1000.0 % 1400.0) / 1400.0
        };
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        let segment = width * 0.3;
        let x = -segment + (width + segment) * phase;
        self.register_access_node(
            node_id,
            Role::ProgressBar,
            "progress",
            Some("indeterminate"),
            false,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("progress node exists")
            .commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(radius),
                color: theme.border,
            },
            DrawCommand::ScissorPush {
                rect: Rect::new(0.0, 0.0, width, height),
            },
            DrawCommand::RoundedRect {
                x,
                y: 0.0,
                width: segment,
                height,
                radius: Radius::all(radius),
                color: theme.accent,
            },
            DrawCommand::ScissorPop,
        ];
        ProgressState {
            value: 0.0,
            fraction: phase,
            indeterminate: true,
        }
    }

    /// Creates a spinner that requests animation frames while visible.
    pub fn spinner(&mut self, id: &str) -> SpinnerState {
        let theme = self.theme.clone();
        // MyGo's spinner is a box `FontSize * 1.25` square that strokes 12
        // spokes a twelfth of a turn apart, each fading 0.85 of the way out
        // behind the one leading (`ui/indicators.go` coreSpinner). Its color
        // is the text color, so the spokes fade into the page, not to the
        // accent.
        let size = theme.font_sizes.body * 1.25;
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(size),
                height: Some(size),
                ..Style::default()
            },
        );
        let spinner_active = !self.reduce_motion;
        if spinner_active {
            self.request_animation_frame();
        }
        // A spoke a twelfth of a turn on every twelfth of 0.9s, as MyGo's
        // frame clock shows them.
        const SPOKES: usize = 12;
        let turn = 900.0_f32;
        let ms = self.frame_time.elapsed().as_secs_f32() * 1000.0 % turn;
        let lead = (ms * SPOKES as f32 / turn).floor() as usize % SPOKES;
        let inner = size * 0.22;
        let outer = size * 0.46;
        let stroke = size * 0.09;
        let center = size / 2.0;
        let mut commands = Vec::new();
        for index in 0..SPOKES {
            // Spoke 0 points up, and each next one a twelfth clockwise.
            let angle = -std::f32::consts::FRAC_PI_2
                + index as f32 * std::f32::consts::TAU / SPOKES as f32;
            let age = (lead + SPOKES - index) % SPOKES;
            let alpha = 1.0 - age as f32 / SPOKES as f32 * 0.85;
            commands.push(DrawCommand::Line {
                x1: center + angle.cos() * inner,
                y1: center + angle.sin() * inner,
                x2: center + angle.cos() * outer,
                y2: center + angle.sin() * outer,
                width: stroke,
                color: Color::rgba(
                    theme.muted.r,
                    theme.muted.g,
                    theme.muted.b,
                    (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
                ),
            });
        }
        self.register_access_node(
            node_id,
            Role::Spinner,
            "loading",
            Some(if spinner_active { "busy" } else { "static" }),
            false,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("spinner node exists")
            .commands = commands;
        SpinnerState {
            // The turn the lead spoke has made, in radians.
            angle: (lead as f32) * std::f32::consts::TAU / SPOKES as f32,
            spinning: spinner_active,
        }
    }

    /// Creates a short text pill.
    pub fn badge(&mut self, label: &str) -> BadgeState {
        // MyGo's Badge (`ui/sidebar.go` coreBadge): a pill padded
        // Space(1.5) at the sides with no vertical padding, radius 999,
        // filled with the text color at 10% over what is behind it, its
        // label FontSize - 2 on a single line.
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body - 2.0;
        let text_width = measure_text_width(label, font_size);
        let text_height = measured_text_height(font_size);
        let padding = theme.spacing.base * 1.5;
        let width = text_width + padding * 2.0;
        let height = text_height;
        let node_id = self.new_node(Style {
            width: Some(width),
            height: Some(height),
            ..Style::default()
        });
        self.register_access_node(node_id, Role::Badge, label, None, false);
        self.nodes
            .get_mut(&node_id)
            .expect("badge node exists")
            .commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(999.0),
                color: Color::rgba(theme.foreground.r, theme.foreground.g, theme.foreground.b, 26),
            },
            DrawCommand::Text {
                x: padding,
                y: 0.0,
                text: label.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: 400,
                color: theme.foreground,
                max_width: Some(text_width),
                alignment: TextAlignment::Center,
            },
        ];
        BadgeState { width, height }
    }

    /// Creates a header that shows or hides content.
    pub fn collapsible(
        &mut self,
        id: &str,
        open: &mut bool,
        title: &str,
        content: impl FnOnce(&mut Context),
    ) -> CollapsibleState {
        // MyGo's Collapsible (`ui/collapsible.go` coreCollapsible): a column
        // holding the trigger, a row padded Space(1) = 4 down, Space(2) = 8
        // after and Space(0.5) = 2 before, gap Space(1) = 4, radius 6, whose
        // hovered fill is the hovered surface. A Space(4) = 16 arrow stroked
        // 1.5 in the muted color points right when closed and turns 90
        // degrees as it opens, then the label. The panel below is padded
        // Space(1) = 4 down, Space(5.5) = 22 before, gap Space(2) = 8.
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        let space = theme.spacing.base;
        let arrow = space * 4.0;
        let pad_v = space;
        let pad_start = space * 0.5;
        let pad_end = space * 2.0;
        let gap = space;
        let text_height = measured_text_height(font_size);
        // The trigger is as high as its tallest child, the label's line box,
        // plus Space(1) = 4 of padding either side.
        let trigger_height = arrow.max(text_height) + pad_v * 2.0;
        let label_width = measure_text_width(title, font_size);
        let trigger_width = (pad_start + arrow + gap + label_width + pad_end).min(width);
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let header_key = format!("{id}:header");
        let header = self.new_named_node(
            &header_key,
            Style {
                width: Some(trigger_width),
                height: Some(trigger_height),
                ..Style::default()
            },
        );
        self.register_interaction(header, header_key.clone());
        self.register_focusable(header, header_key.clone());
        let hovered = self.pointer_in_previous(&header_key);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(header_key.clone());
        }
        let focused = self.focused_id.as_deref() == Some(header_key.as_str());
        let toggled = self.clicked_in_previous(&header_key)
            || (focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space)));
        if toggled {
            *open = !*open;
        }
        let mut commands = Vec::new();
        if hovered {
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: trigger_width,
                height: trigger_height,
                radius: Radius::all(6.0),
                color: theme.surface_hover,
            });
        }
        // The arrow is a chevron pointing right, turned clockwise as the
        // disclosure opens (`disclosurePath`): its arms run from
        // (-d, -2d) through (d, 0) to (-d, 2d) around its box's center.
        let cx = pad_start + arrow / 2.0;
        let cy = trigger_height / 2.0;
        let d = arrow / 8.0;
        let degrees = if *open { 90.0_f32 } else { 0.0 };
        let (sin, cos) = (degrees * std::f32::consts::PI / 180.0).sin_cos();
        let at = |x: f32, y: f32| (cx + x * cos - y * sin, cy + x * sin + y * cos);
        let (ax, ay) = at(-d, -2.0 * d);
        let (bx, by) = at(d, 0.0);
        let (ex, ey) = at(-d, 2.0 * d);
        commands.push(DrawCommand::SvgPath {
            x: 0.0,
            y: 0.0,
            width: trigger_width,
            height: trigger_height,
            path: format!("M{ax} {ay}L{bx} {by}L{ex} {ey}"),
            fill: None,
            stroke: Some(theme.muted),
            stroke_width: 1.5,
        });
        commands.push(DrawCommand::Text {
            x: pad_start + arrow + gap,
            y: (trigger_height - text_height) / 2.0,
            text: title.to_string(),
            font_size,
            italic: false,
            mono: false,
            family: None,
            weight: 400,
            color: theme.foreground,
            max_width: Some(label_width.max(1.0)),
            alignment: TextAlignment::Start,
        });
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, trigger_width, trigger_height, theme.accent));
        }
        self.register_access_node(
            header,
            Role::Disclosure,
            title,
            Some(if *open { "expanded" } else { "collapsed" }),
            focused,
        );
        self.nodes
            .get_mut(&header)
            .expect("collapsible header exists")
            .commands = commands;
        if *open {
            // `Padding(v, h)` pads both edges of the axis, so the panel is
            // padded Space(1) = 4 above and below.
            column(self)
                .padding_each(pad_v, 0.0, pad_v, space * 5.5)
                .gap(space * 2.0)
                .children(content);
        }
        self.pop_parent();
        self.register_access_node(root, Role::Group, id, None, false);
        CollapsibleState {
            changed: toggled,
            open: *open,
            focused,
            hovered,
        }
    }

    /// Creates a disclosure whose trigger the caller draws, as MyGo's
    /// `CollapsibleBase`: a chevron and the label, the chevron turning as
    /// the panel opens. `content` builds the panel.
    pub fn collapsible_custom(
        &mut self,
        id: &str,
        open: &mut bool,
        title: &str,
        content: impl FnOnce(&mut Context),
    ) -> CollapsibleState {
        // MyGo's `CollapsibleBase` with the demo's own trigger
        // (`ui/collapsible.go`, `website/docs-demo/containers.go`): a row
        // gap Space(1.5) = 6 holding a chevron icon that turns 90 degrees as
        // the panel opens, then the label; the panel is padded
        // Space(1) = 4 down and Space(5.5) = 22 before, with a gap of
        // Space(2) = 8.
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        let space = theme.spacing.base;
        let gap = space * 1.5;
        // MyGo's chevron is an Icon at the theme's font size, an em
        // (`website/docs-demo/containers.go` containersChevron).
        let arrow = font_size;
        let text_height = measured_text_height(font_size);
        let label_width = measure_text_width(title, font_size);
        let trigger_width = (arrow + gap + label_width).min(width);
        let trigger_height = arrow.max(text_height);
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let header_key = format!("{id}:header");
        let header = self.new_named_node(
            &header_key,
            Style {
                width: Some(trigger_width),
                height: Some(trigger_height),
                ..Style::default()
            },
        );
        self.register_interaction(header, header_key.clone());
        self.register_focusable(header, header_key.clone());
        let hovered = self.pointer_in_previous(&header_key);
        if hovered && self.input.pointer_down {
            self.focused_id = Some(header_key.clone());
        }
        let focused = self.focused_id.as_deref() == Some(header_key.as_str());
        let toggled = self.clicked_in_previous(&header_key)
            || (focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space)));
        if toggled {
            *open = !*open;
        }
        // The chevron is MyGo's own path, `m9 18 6-6-6-6` in a 24-unit
        // view box, scaled into the icon's em and turned by 90 degrees as
        // the panel opens (`Icon.Rotate(90 * p.Progress())`).
        let degrees = if *open { 90.0_f32 } else { 0.0 };
        let em_scale = arrow / 24.0;
        // A rotation about the view box's middle: the affine applies the
        // rotation before its translation, so the translation is the
        // middle less where the rotation puts it.
        let (sin, cos) = degrees.to_radians().sin_cos();
        let middle = 12.0_f32;
        let translation = Point::new(
            middle - (cos * middle - sin * middle),
            middle - (sin * middle + cos * middle),
        );
        let mut commands = vec![
            DrawCommand::TransformPush {
                transform: Transform {
                    translation: Point::new(0.0, 0.0),
                    scale: Point::new(em_scale, em_scale),
                    rotation: 0.0,
                },
            },
            DrawCommand::TransformPush {
                transform: Transform {
                    translation,
                    scale: Point::new(1.0, 1.0),
                    rotation: degrees.to_radians(),
                },
            },
            DrawCommand::SvgPath {
                x: 0.0,
                y: 0.0,
                width: 24.0,
                height: 24.0,
                path: "M9 18 15 12 9 6".to_string(),
                fill: None,
                stroke: Some(theme.foreground),
                stroke_width: 2.0,
            },
            DrawCommand::TransformPop,
            DrawCommand::TransformPop,
        ];
        commands.push(DrawCommand::Text {
            x: arrow + gap,
            y: (trigger_height - text_height) / 2.0,
            text: title.to_string(),
            font_size,
            italic: false,
            mono: false,
            family: None,
            weight: 400,
            color: theme.foreground,
            max_width: Some(label_width.max(1.0)),
            alignment: TextAlignment::Start,
        });
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, trigger_width, trigger_height, theme.accent));
        }
        self.register_access_node(
            header,
            Role::Disclosure,
            title,
            Some(if *open { "expanded" } else { "collapsed" }),
            focused,
        );
        self.nodes
            .get_mut(&header)
            .expect("collapsible header exists")
            .commands = commands;
        if *open {
            column(self)
                .padding_each(space, 0.0, 0.0, space * 5.5)
                .gap(space * 2.0)
                .children(content);
        }
        self.pop_parent();
        self.register_access_node(root, Role::Group, id, None, false);
        CollapsibleState {
            changed: toggled,
            open: *open,
            focused,
            hovered,
        }
    }

    /// Creates a button that owns an anchored menu.
    /// Draws MyGo's menu button trigger: a surface button with the label
    /// left and the chevron that says it opens a menu right.
    fn chevron_button(&mut self, id: &str, label: &str) -> ButtonState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let label_width = measure_text_width(label, font_size);
        let padding_x = 15.0;
        let width = padding_x * 2.0 + label_width + 6.0 + 10.0;
        let height = measured_text_height(font_size) + 14.0;
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.register_focusable(node_id, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let pressed = hovered && self.input.pointer_down;
        if pressed {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let clicked = self.clicked_in_previous(id)
            || (focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space)));
        let face = if pressed {
            theme.surface_pressed
        } else if hovered {
            theme.surface_hover
        } else {
            theme.surface
        };
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.fill = Some((face, Radius::all(6.0)));
            node.stroke = Some((theme.border, Radius::all(6.0)));
        }
        let chevron_x = width - padding_x - 10.0;
        let chevron_y = (height - 10.0) / 2.0;
        let mut commands = vec![
            DrawCommand::Text {
                x: padding_x,
                y: (height - measured_text_height(font_size)) / 2.0,
                text: label.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: 400,
                color: theme.foreground,
                max_width: Some(label_width.max(1.0)),
                alignment: TextAlignment::Start,
            },
            DrawCommand::SvgPath {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                path: format!(
                    "M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}",
                    chevron_x + 1.0,
                    chevron_y + 3.0,
                    chevron_x + 5.0,
                    chevron_y + 7.0,
                    chevron_x + 9.0,
                    chevron_y + 3.0
                ),
                fill: None,
                stroke: Some(theme.muted),
                stroke_width: 1.5,
            },
        ];
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.register_access_node(node_id, Role::Button, label, None, focused);
        self.nodes
            .get_mut(&node_id)
            .expect("menu trigger exists")
            .commands = commands;
        ButtonState {
            id: id.to_string(),
            clicked,
            pressed,
            focused,
            hovered,
            bounds: self.previous_regions.get(id).map(|region| region.bounds),
        }
    }

    pub fn menu_button(
        &mut self,
        id: &str,
        label: &str,
        state: &mut MenuState,
        items: &[&str],
    ) -> MenuButtonState {
        let trigger = self.chevron_button(id, label);
        let menu_id = format!("{id}:menu");
        let was_open = state.open.open;
        if items.is_empty() {
            if was_open {
                self.close_overlay_runtime(&menu_id);
            }
            state.open.open = false;
            return MenuButtonState {
                open: false,
                trigger_clicked: trigger.clicked,
                selected: None,
                closed: was_open,
                focused: trigger.focused,
                hovered: trigger.hovered,
            };
        }
        let mut open = was_open;
        if trigger.clicked {
            open = !open;
            state.highlight = 0;
        }
        state.open.open = open;
        let mut selected = None;
        let mut closed = false;
        if open {
            let anchor = trigger
                .bounds
                .unwrap_or_else(|| Rect::new(0.0, 0.0, 220.0, 0.0));
            let (still_open, did_close, chosen) = self.anchored_menu(
                &menu_id,
                &mut state.open,
                &mut state.highlight,
                anchor,
                items,
                was_open,
            );
            selected = chosen;
            closed = did_close;
            open = still_open;
        } else if was_open {
            self.close_overlay_runtime(&menu_id);
            closed = true;
        }
        MenuButtonState {
            open,
            trigger_clicked: trigger.clicked,
            selected,
            closed,
            focused: trigger.focused,
            hovered: trigger.hovered,
        }
    }

    /// Creates a system-style context menu opened by a secondary press.
    pub fn context_menu(
        &mut self,
        id: &str,
        state: &mut ContextMenuState,
        area: Rect,
        items: &[&str],
    ) -> ContextMenuResponse {
        let menu_id = format!("{id}:menu");
        let was_open = state.open.open;
        let mut opened = false;
        if !was_open && !items.is_empty() {
            if let Some(point) = self.input.secondary_pressed {
                if self.pointer_available(self.current_layer) && rect_contains(area, point) {
                    state.open.open = true;
                    state.highlight = 0;
                    state.anchor = Some(point);
                    opened = true;
                }
            }
        }
        let open = state.open.open;
        let mut selected = None;
        let mut closed = false;
        if open {
            let anchor = state.anchor.unwrap_or_else(|| {
                Point::new(area.x + area.width / 2.0, area.y + area.height / 2.0)
            });
            let anchor = Rect::new(anchor.x, anchor.y, 0.0, 0.0);
            let (_, did_close, chosen) = self.anchored_menu(
                &menu_id,
                &mut state.open,
                &mut state.highlight,
                anchor,
                items,
                was_open,
            );
            selected = chosen;
            closed = did_close;
        } else if was_open {
            self.close_overlay_runtime(&menu_id);
            closed = true;
        }
        ContextMenuResponse {
            open: state.open.open,
            opened,
            selected,
            closed,
        }
    }

    /// Builds one anchored menu and owns its keyboard highlight and selection.
    fn anchored_menu(
        &mut self,
        id: &str,
        state: &mut OverlayState,
        highlight: &mut usize,
        anchor: Rect,
        items: &[&str],
        was_open: bool,
    ) -> (bool, bool, Option<usize>) {
        let mut selected = None;
        let mut local_highlight = (*highlight).min(items.len().saturating_sub(1));
        let response = self.overlay(id, state, anchor, |context| {
            if items.is_empty() {
                return;
            }
            if was_open {
                if context.key_pressed(Key::ArrowDown) {
                    local_highlight = (local_highlight + 1).min(items.len() - 1);
                }
                if context.key_pressed(Key::ArrowUp) {
                    local_highlight = local_highlight.saturating_sub(1);
                }
            }
            context.focused_id = Some(format!("{id}:item:{local_highlight}"));
            for (index, item) in items.iter().enumerate() {
                if context.menu_item(&format!("{id}:item:{index}"), item) {
                    selected = Some(index);
                }
            }
            if selected.is_none()
                && was_open
                && (context.key_pressed(Key::Enter) || context.key_pressed(Key::Space))
            {
                selected = Some(local_highlight);
            }
        });
        *highlight = local_highlight;
        let open = response.open && selected.is_none();
        let closed = response.closed || selected.is_some();
        if selected.is_some() {
            self.close_overlay_runtime(id);
            state.open = false;
        }
        (open, closed, selected)
    }

    fn menu_item(&mut self, id: &str, label: &str) -> bool {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        let height = measured_text_height(font_size) + theme.spacing.xs * 2.0;
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.register_focusable(node_id, id.to_string());
        let hovered = self.pointer_in_previous(id);
        let clicked = self.clicked_in_previous(id);
        let focused = self.focused_id.as_deref() == Some(id);
        let background = if focused {
            mix_color(theme.surface, theme.accent, 0.24)
        } else if hovered {
            mix_color(theme.surface, theme.foreground, 0.08)
        } else {
            theme.surface
        };
        let text_color = if focused {
            theme.accent
        } else {
            theme.foreground
        };
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(4.0),
                color: background,
            },
            DrawCommand::Text {
                x: theme.spacing.sm,
                y: theme.spacing.xs,
                text: label.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: if focused { 600 } else { 400 },
                color: text_color,
                max_width: Some((width - theme.spacing.sm * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            },
        ];
        if self.shows_focus_ring(focused) {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.register_access_node(
            node_id,
            Role::Option,
            label,
            Some(if focused { "focused" } else { "normal" }),
            focused,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("menu item node exists")
            .commands = commands;
        clicked
    }

    /// Creates an icon-only button that opens a menu.
    pub fn menu_icon_button(
        &mut self,
        id: &str,
        icon_name: &str,
        state: &mut MenuState,
        items: &[&str],
    ) -> MenuButtonState {
        let trigger = self.id(id).icon_button(icon_name);
        self.menu_with_trigger(id, trigger, state, items)
    }
    fn menu_with_trigger(
        &mut self,
        id: &str,
        trigger: ButtonState,
        state: &mut MenuState,
        items: &[&str],
    ) -> MenuButtonState {
        let menu_id = format!("{id}:menu");
        let was_open = state.open.open;
        if items.is_empty() {
            if was_open {
                self.close_overlay_runtime(&menu_id);
            }
            state.open.open = false;
            return MenuButtonState {
                open: false,
                trigger_clicked: trigger.clicked,
                selected: None,
                closed: was_open,
                focused: trigger.focused,
                hovered: trigger.hovered,
            };
        }
        let mut open = was_open;
        if trigger.clicked {
            open = !open;
            state.highlight = 0;
        }
        state.open.open = open;
        let mut selected = None;
        let mut closed = false;
        if open {
            let anchor = trigger
                .bounds
                .unwrap_or_else(|| Rect::new(0.0, 0.0, 220.0, 0.0));
            let (still_open, did_close, chosen) = self.anchored_menu(
                &menu_id,
                &mut state.open,
                &mut state.highlight,
                anchor,
                items,
                was_open,
            );
            selected = chosen;
            closed = did_close;
            open = still_open;
        } else if was_open {
            self.close_overlay_runtime(&menu_id);
            closed = true;
        }
        MenuButtonState {
            open,
            trigger_clicked: trigger.clicked,
            selected,
            closed,
            focused: trigger.focused,
            hovered: trigger.hovered,
        }
    }
    /// Opens a context menu over the previous-frame bounds of an
    /// interactive node registered under `id`.
    pub fn node_context_menu(
        &mut self,
        id: &str,
        state: &mut ContextMenuState,
        items: &[&str],
    ) -> ContextMenuResponse {
        let area = self
            .previous_regions
            .get(id)
            .map(|region| region.bounds)
            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
        self.context_menu(id, state, area, items)
    }

    /// Creates a non-interactive tooltip after a hover or focus delay.
    pub fn tooltip_with_delay(
        &mut self,
        id: &str,
        anchor: Rect,
        target: Option<&str>,
        text: &str,
        delay: Duration,
    ) -> TooltipState {
        let now = self.frame_time;
        let hovering = self.pointer_available(self.current_layer)
            && self
                .input
                .pointer
                .map(|point| rect_contains(anchor, point))
                .unwrap_or(false);
        let focused = target
            .map(|target| self.focused_id.as_deref() == Some(target))
            .unwrap_or(false);
        let active = hovering || focused;
        let hovered_since = match self.widget_states.get(id) {
            Some(WidgetState::Tooltip { hovered_since }) => *hovered_since,
            _ => None,
        };
        let hovered_since = if active {
            hovered_since.or(Some(now))
        } else {
            None
        };
        self.widget_states
            .insert(id.to_string(), WidgetState::Tooltip { hovered_since });
        let elapsed = hovered_since
            .map(|since| now.saturating_duration_since(since))
            .unwrap_or_default();
        let visible = active && elapsed >= delay;
        if active && !visible {
            self.request_animation_frame();
        }
        if visible {
            let theme = self.theme.clone();
            let font_size = theme.font_sizes.small;
            let padding_x = theme.spacing.sm;
            let padding_y = theme.spacing.xs;
            let text_width = measure_text_width(text, font_size);
            let text_height = measured_text_height(font_size);
            let width = text_width + padding_x * 2.0;
            let height = text_height + padding_y * 2.0;
            let x = anchor
                .x
                .clamp(0.0, (self.available_size.width - width).max(0.0));
            let gap = theme.spacing.xs;
            let mut y = anchor.y + anchor.height + gap;
            if y + height > self.available_size.height && anchor.y - height - gap >= 0.0 {
                y = anchor.y - height - gap;
            }
            let previous_layer = self.current_layer;
            let saved_parents = std::mem::take(&mut self.parent_stack);
            let layer = self.allocate_overlay_layer();
            self.current_layer = layer;
            let node_id = self.new_node(Style {
                width: Some(width),
                height: Some(height),
                position: Position::Absolute,
                left: Some(x),
                top: Some(y),
                ..Style::default()
            });
            self.parent_stack = saved_parents;
            self.current_layer = previous_layer;
            self.register_access_node(node_id, Role::Label, text, None, false);
            self.nodes
                .get_mut(&node_id)
                .expect("tooltip node exists")
                .commands = vec![
                DrawCommand::RoundedRect {
                    x: -1.0,
                    y: 3.0,
                    width: width + 2.0,
                    height: height + 2.0,
                    radius: Radius::all(8.0),
                    color: Color::rgba(0, 0, 0, 24),
                },
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                    radius: Radius::all(6.0),
                    color: theme.border,
                },
                DrawCommand::RoundedRect {
                    x: 1.0,
                    y: 1.0,
                    width: (width - 2.0).max(1.0),
                    height: (height - 2.0).max(1.0),
                    radius: Radius::all(5.0),
                    color: theme.surface,
                },
                DrawCommand::Text {
                    x: padding_x,
                    y: padding_y,
                    text: text.to_string(),
                    font_size,
                    italic: false,
                    mono: false,
                    family: None,
                    weight: 400,
                    color: theme.foreground,
                    max_width: Some(text_width.max(1.0)),
                    alignment: TextAlignment::Start,
                },
            ];
        }
        TooltipState {
            visible,
            hovering,
            focused,
            elapsed,
        }
    }

    /// Paints a caller-owned toast queue and expires finished entries.
    pub fn toasts(&mut self, id: &str, queue: &mut ToastQueue) -> ToastResponse {
        let theme = self.theme.clone();
        let now = self.frame_time;
        let margin = theme.spacing.md;
        let width = (self.available_size.width - margin * 2.0).clamp(1.0, 320.0);
        let x = self.available_size.width - width - margin;
        let gap = theme.spacing.xs;
        let font_size = theme.font_sizes.body;
        let mut events = Vec::new();
        let mut remove = vec![false; queue.entries.len()];
        let mut y = margin;
        let previous_layer = self.current_layer;
        let saved_parents = std::mem::take(&mut self.parent_stack);
        let layer = 1;
        self.current_layer = layer;
        for (index, toast) in queue.entries.iter().enumerate() {
            let started_at = toast.started_at.unwrap_or(now);
            let timed_out = now.saturating_duration_since(started_at) >= toast.duration;
            if timed_out {
                events.push(ToastEvent {
                    index,
                    kind: ToastDismissal::Timeout,
                    action: None,
                });
                remove[index] = true;
                continue;
            }
            let toast_id = format!("{id}:toast:{}", toast.id);
            let node_id = self.new_node(Style {
                width: Some(width),
                position: Position::Absolute,
                left: Some(x),
                top: Some(y),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: Edges::symmetric(theme.spacing.sm, theme.spacing.md),
                gap: theme.spacing.sm,
                ..Style::default()
            });
            self.push_parent(node_id);
            let action_width = toast
                .action
                .as_deref()
                .map(|action| measure_text_width(action, font_size) + theme.spacing.md * 2.0)
                .unwrap_or(0.0);
            let close_width = measure_text_width("x", font_size) + theme.spacing.md * 2.0;
            let gaps = if toast.action.is_some() {
                theme.spacing.sm * 2.0
            } else {
                theme.spacing.sm
            };
            let text_width =
                (width - theme.spacing.md * 2.0 - action_width - close_width - gaps).max(40.0);
            let text_id = self.new_named_node(
                &format!("{toast_id}:message"),
                Style {
                    flex_grow: 1.0,
                    ..Style::default()
                },
            );
            self.update_text(
                text_id,
                &toast.message,
                font_size,
                400,
                theme.foreground,
                Some(text_width),
                TextAlignment::Start,
                0.0,
                false,
                false,
                None,
            );
            self.register_access_node(text_id, Role::Label, &toast.message, None, false);
            let mut dismissal = None;
            if let Some(action) = toast.action.as_deref() {
                let action_key = format!("{toast_id}:action");
                if self.id(&action_key).button(action).clicked {
                    dismissal = Some(ToastDismissal::Action);
                }
            }
            if dismissal.is_none() {
                let close_key = format!("{toast_id}:close");
                if self.id(&close_key).button("x").clicked {
                    dismissal = Some(ToastDismissal::Close);
                }
            }
            self.pop_parent();
            let height =
                (self.estimated_content_height(node_id, width) + theme.spacing.sm).max(36.0);
            let node = self.nodes.get_mut(&node_id).expect("toast node exists");
            node.style.top = Some(y);
            node.style.height = Some(height);
            node.commands = vec![
                DrawCommand::RoundedRect {
                    x: -2.0,
                    y: 7.0,
                    width: width + 4.0,
                    height: height + 4.0,
                    radius: Radius::all(11.0),
                    color: Color::rgba(0, 0, 0, 18),
                },
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 3.0,
                    width,
                    height,
                    radius: Radius::all(9.0),
                    color: Color::rgba(0, 0, 0, 32),
                },
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                    radius: Radius::all(8.0),
                    color: theme.border,
                },
                DrawCommand::RoundedRect {
                    x: 1.0,
                    y: 1.0,
                    width: (width - 2.0).max(1.0),
                    height: (height - 2.0).max(1.0),
                    radius: Radius::all(7.0),
                    color: theme.surface,
                },
            ];
            self.register_access_node(node_id, Role::Group, "toast", Some(&toast.message), false);
            y += height + gap;
            if let Some(kind) = dismissal {
                events.push(ToastEvent {
                    index,
                    kind,
                    action: if kind == ToastDismissal::Action {
                        toast.action.clone()
                    } else {
                        None
                    },
                });
                remove[index] = true;
            }
        }
        self.parent_stack = saved_parents;
        self.current_layer = previous_layer;
        for index in (0..remove.len()).rev() {
            if remove[index] {
                queue.entries.remove(index);
            }
        }
        if !queue.entries.is_empty() {
            self.request_animation_frame();
        }
        ToastResponse {
            events,
            visible: queue.entries.len(),
        }
    }

    /// Creates a centered modal dialog with overlay focus and blocking rules.
    pub fn dialog(
        &mut self,
        id: &str,
        open: &mut bool,
        width: f32,
        content: impl FnOnce(&mut Context) -> DialogAction,
    ) -> DialogResponse {
        let mut runtime = self.overlay_runtimes.remove(id).unwrap_or_default();
        if !*open {
            let closed = runtime.was_open;
            runtime.was_open = false;
            if closed {
                if let Some(focus) = runtime.restore_focus.take() {
                    self.focused_id = Some(focus);
                }
            }
            self.overlay_runtimes.insert(id.to_string(), runtime);
            return DialogResponse {
                open: false,
                closed,
                ..DialogResponse::default()
            };
        }
        let opening = !runtime.was_open;
        if opening {
            runtime.restore_focus = self.focused_id.clone();
            self.focused_id = None;
        }
        if self.key_pressed(Key::Escape) {
            *open = false;
            runtime.was_open = false;
            if let Some(focus) = runtime.restore_focus.take() {
                self.focused_id = Some(focus);
            }
            self.overlay_runtimes.insert(id.to_string(), runtime);
            return DialogResponse {
                open: false,
                closed: true,
                escape_pressed: true,
                ..DialogResponse::default()
            };
        }

        let theme = self.theme.clone();
        let layer = self.allocate_overlay_layer();
        let previous_layer = self.current_layer;
        self.current_layer = layer;
        let saved_parents = std::mem::take(&mut self.parent_stack);

        let backdrop = self.new_node(Style {
            width: Some(self.available_size.width),
            height: Some(self.available_size.height),
            position: Position::Absolute,
            left: Some(0.0),
            top: Some(0.0),
            ..Style::default()
        });
        self.nodes
            .get_mut(&backdrop)
            .expect("dialog backdrop exists")
            .commands = vec![DrawCommand::Rect {
            x: 0.0,
            y: 0.0,
            width: self.available_size.width,
            height: self.available_size.height,
            color: Color::rgba(0, 0, 0, 132),
        }];

        let max_width = (self.available_size.width - theme.spacing.xl).max(1.0);
        let requested = if width.is_finite() && width > 0.0 {
            width
        } else {
            360.0
        };
        let min_width = 200.0_f32.min(max_width);
        let panel_width = requested.clamp(min_width, max_width);
        let x = (self.available_size.width - panel_width) / 2.0;
        let panel_id = self.new_named_node(
            id,
            Style {
                width: Some(panel_width),
                position: Position::Absolute,
                left: Some(x),
                top: Some(0.0),
                flex_direction: FlexDirection::Column,
                padding: Edges::all(theme.spacing.md),
                gap: theme.spacing.sm,
                ..Style::default()
            },
        );
        self.push_parent(panel_id);
        let action = content(self);
        self.pop_parent();
        self.parent_stack = saved_parents;
        self.current_layer = previous_layer;

        let max_height = (self.available_size.height - theme.spacing.xl).max(1.0);
        let height = (self.estimated_content_height(panel_id, panel_width) + theme.spacing.md)
            .max(1.0)
            .min(max_height);
        let top = ((self.available_size.height - height) / 2.0).max(0.0);
        let node = self.nodes.get_mut(&panel_id).expect("dialog panel exists");
        node.style.top = Some(top);
        node.style.height = Some(height);
        node.scroll = Some(ScrollNode {
            offset: 0.0,
            viewport_height: height,
        });
        node.commands = vec![
            DrawCommand::RoundedRect {
                x: 2.0,
                y: 6.0,
                width: panel_width,
                height,
                radius: Radius::all(14.0),
                color: Color::rgba(0, 0, 0, 96),
            },
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: panel_width,
                height,
                radius: Radius::all(12.0),
                color: theme.border,
            },
            DrawCommand::RoundedRect {
                x: 1.0,
                y: 1.0,
                width: (panel_width - 2.0).max(1.0),
                height: (height - 2.0).max(1.0),
                radius: Radius::all(11.0),
                color: theme.surface,
            },
        ];
        self.register_access_node(panel_id, Role::Popup, id, None, false);
        self.frame_overlays.push((panel_id, id.to_string(), layer));
        if opening {
            self.pending_focus_layer = Some(layer);
        }
        runtime.was_open = true;
        self.overlay_runtimes.insert(id.to_string(), runtime);

        let confirmed = action == DialogAction::Confirm;
        let cancelled = action == DialogAction::Cancel;
        if action != DialogAction::None {
            *open = false;
            self.close_overlay_runtime(id);
            return DialogResponse {
                open: false,
                closed: true,
                confirmed,
                cancelled,
                escape_pressed: false,
            };
        }
        DialogResponse {
            open: true,
            closed: false,
            confirmed: false,
            cancelled: false,
            escape_pressed: false,
        }
    }

    /// Creates a standard title/body/confirm/cancel alert dialog.
    pub fn alert_dialog(
        &mut self,
        id: &str,
        open: &mut bool,
        title: &str,
        message: &str,
        confirm_label: &str,
        cancel_label: &str,
    ) -> DialogResponse {
        let confirm_label = if confirm_label.is_empty() {
            "OK"
        } else {
            confirm_label
        };
        let cancel_label = if cancel_label.is_empty() {
            "Cancel"
        } else {
            cancel_label
        };
        let mut action = DialogAction::None;
        let mut response = self.dialog(id, open, 380.0, |context| {
            let theme = context.theme().clone();
            let content_width = context.current_content_width();
            context
                .text(title)
                .font_size(theme.font_sizes.body + 4.0)
                .weight(600)
                .max_width(content_width);
            if !message.is_empty() {
                context
                    .text(message)
                    .font_size(theme.font_sizes.body)
                    .color(theme.muted)
                    .max_width(content_width);
            }
            context.row().gap(theme.spacing.sm).children(|context| {
                context.spacer();
                if context
                    .id(&format!("{id}:cancel"))
                    .button(cancel_label)
                    .clicked
                {
                    action = DialogAction::Cancel;
                }
                if context
                    .id(&format!("{id}:confirm"))
                    .button(confirm_label)
                    .clicked
                {
                    action = DialogAction::Confirm;
                }
            });
            action
        });
        if response.escape_pressed {
            response.cancelled = true;
        }
        response
    }

    /// Creates indented tree rows.
    pub fn tree(
        &mut self,
        id: &str,
        state: &mut TreeState,
        nodes: &[TreeNode<'_>],
        expanded: &mut Vec<String>,
    ) -> TreeResponse {
        self.tree_impl(id, state, nodes, expanded, false)
    }

    /// Creates a panel-style outline over the tree interaction.
    pub fn outline(
        &mut self,
        id: &str,
        state: &mut TreeState,
        nodes: &[TreeNode<'_>],
        expanded: &mut Vec<String>,
    ) -> TreeResponse {
        self.tree_impl(id, state, nodes, expanded, true)
    }

    fn tree_impl(
        &mut self,
        id: &str,
        state: &mut TreeState,
        nodes: &[TreeNode<'_>],
        expanded: &mut Vec<String>,
        outline_style: bool,
    ) -> TreeResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let row_height = 27.0_f32;
        let total_height = nodes.len() as f32 * row_height;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(total_height),
                ..Style::default()
            },
        );
        if outline_style {
            self.nodes
                .get_mut(&root)
                .expect("outline root exists")
                .commands = vec![DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height: total_height,
                radius: Radius::all(6.0),
                color: theme.surface,
            }];
        }
        let mut toggled_id = None;
        let mut toggled_focus = None;
        for (index, node) in nodes.iter().enumerate() {
            if !node.has_children {
                continue;
            }
            let key = format!("{id}:toggle:{index}");
            if self.clicked_in_previous(&key) {
                if expanded.iter().any(|open| open == node.id) {
                    expanded.retain(|open| open != node.id);
                } else {
                    expanded.push(node.id.to_string());
                }
                toggled_id = Some(node.id.to_string());
                toggled_focus = Some(key);
                break;
            }
        }
        let previous_selected = state.selected_id.clone();
        let mut clicked_id = None;
        let mut selected_focus = None;
        for (index, node) in nodes.iter().enumerate() {
            let key = format!("{id}:row:{index}");
            if self.clicked_in_previous(&key) {
                state.selected_id.clear();
                state.selected_id.push_str(node.id);
                clicked_id = Some(node.id.to_string());
                selected_focus = Some(key);
                break;
            }
        }
        let changed = state.selected_id != previous_selected;
        let selected_id = state.selected_id.clone();
        if let Some(key) = selected_focus.or(toggled_focus) {
            self.focused_id = Some(key);
        }

        for (index, node) in nodes.iter().enumerate() {
            let indent = (node.depth as f32 * theme.spacing.md).min((width - 20.0).max(0.0));
            let top = index as f32 * row_height;
            let expanded_now = expanded.iter().any(|open| open == node.id);
            if node.has_children {
                let key = format!("{id}:toggle:{index}");
                let toggle = self.new_named_node(
                    &key,
                    Style {
                        width: Some(20.0),
                        height: Some(row_height),
                        position: Position::Absolute,
                        left: Some(indent),
                        top: Some(top),
                        ..Style::default()
                    },
                );
                self.register_interaction(toggle, key.clone());
                self.register_focusable(toggle, key.clone());
                let hovered = self.pointer_in_previous(&key);
                let focused = self.focused_id.as_deref() == Some(key.as_str());
                let center = row_height / 2.0;
                let arrow = if expanded_now {
                    [
                        (6.0, center - 3.0, 10.0, center + 1.0),
                        (10.0, center + 1.0, 14.0, center - 3.0),
                    ]
                } else {
                    [
                        (7.0, center - 4.0, 11.0, center),
                        (11.0, center, 7.0, center + 4.0),
                    ]
                };
                let mut commands = Vec::new();
                for (x1, y1, x2, y2) in arrow {
                    commands.push(DrawCommand::Line {
                        x1,
                        y1,
                        x2,
                        y2,
                        width: 2.0,
                        color: if hovered || focused {
                            theme.accent
                        } else {
                            theme.muted
                        },
                    });
                }
                if self.shows_focus_ring(focused) {
                    commands.extend(focus_ring(0.0, 0.0, 20.0, row_height, theme.accent));
                }
                self.register_access_node(
                    toggle,
                    Role::Button,
                    node.label,
                    Some(if expanded_now {
                        "expanded"
                    } else {
                        "collapsed"
                    }),
                    focused,
                );
                self.nodes
                    .get_mut(&toggle)
                    .expect("tree toggle exists")
                    .commands = commands;
            }
            let row_left = indent + 20.0;
            let row_width = (width - row_left).max(1.0);
            let key = format!("{id}:row:{index}");
            let row = self.new_named_node(
                &key,
                Style {
                    width: Some(row_width),
                    height: Some(row_height),
                    position: Position::Absolute,
                    left: Some(row_left),
                    top: Some(top),
                    ..Style::default()
                },
            );
            self.register_interaction(row, key.clone());
            self.register_focusable(row, key.clone());
            let hovered = self.pointer_in_previous(&key);
            let focused = self.focused_id.as_deref() == Some(key.as_str());
            let selected = selected_id == node.id;
            let mut commands = Vec::new();
            if selected || hovered {
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: row_width,
                    height: row_height,
                    radius: Radius::all(6.0),
                    color: if selected {
                        theme.accent
                    } else {
                        mix_color(theme.surface, theme.foreground, 0.06)
                    },
                });
            }
            commands.push(DrawCommand::Text {
                x: theme.spacing.sm,
                y: theme.spacing.xs,
                text: node.label.to_string(),
                font_size: theme.font_sizes.body,
                italic: false,
                mono: false,
                family: None,
                weight: if selected { 600 } else { 400 },
                color: if selected {
                    theme.on_accent
                } else if hovered {
                    theme.foreground
                } else {
                    theme.muted
                },
                max_width: Some((row_width - theme.spacing.sm * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
            if self.shows_focus_ring(focused) {
                commands.extend(focus_ring(0.0, 0.0, row_width, row_height, theme.accent));
            }
            self.register_access_node(
                row,
                Role::Option,
                node.label,
                Some(if selected { "selected" } else { "unselected" }),
                focused,
            );
            self.nodes.get_mut(&row).expect("tree row exists").commands = commands;
        }
        self.register_access_node(root, Role::Group, id, None, false);
        TreeResponse {
            selected_id,
            changed,
            clicked_id,
            toggled_id,
        }
    }

    /// Creates a table with sortable headers and row selection.
    pub fn table(
        &mut self,
        id: &str,
        state: &mut TableState,
        columns: &[TableColumn<'_>],
        rows: &[TableRow<'_>],
    ) -> TableResponse {
        let theme = self.theme.clone();
        // MyGo's compact table grid uses a 31px row; keep the text baseline
        // from the spacing token but give the row its own minimum height.
        let header_height = (measured_text_height(theme.font_sizes.body) + theme.spacing.xs * 2.0).max(31.0);
        let row_height = header_height;
        let column_width = |column: &TableColumn<'_>| {
            if column.width.is_finite() && column.width > 0.0 {
                column.width
            } else {
                100.0
            }
        };
        let total_width = columns.iter().map(column_width).sum::<f32>().max(1.0);
        let mut sort = None;
        for (index, _) in columns.iter().enumerate() {
            let key = format!("{id}:head:{index}");
            let clicked = self.clicked_in_previous(&key);
            let focused = self.focused_id.as_deref() == Some(key.as_str());
            let pressed = focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space));
            if clicked || pressed {
                let direction = if state.sort_column == Some(index) {
                    match state.sort_direction {
                        SortDirection::Ascending => SortDirection::Descending,
                        SortDirection::Descending => SortDirection::Ascending,
                    }
                } else {
                    SortDirection::Ascending
                };
                state.sort_column = Some(index);
                state.sort_direction = direction;
                self.focused_id = Some(key);
                sort = Some(TableSort {
                    column: index,
                    direction,
                });
                break;
            }
        }
        let previous_selected = state.selected_id.clone();
        let mut clicked_id = None;
        for (index, row) in rows.iter().enumerate() {
            let key = format!("{id}:row:{index}");
            if self.clicked_in_previous(&key) {
                state.selected_id.clear();
                state.selected_id.push_str(row.id);
                self.focused_id = Some(key);
                clicked_id = Some(row.id.to_string());
                break;
            }
        }
        let changed = state.selected_id != previous_selected;
        let selected_id = state.selected_id.clone();

        let total_height = header_height + rows.len() as f32 * row_height;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(total_width),
                height: Some(total_height),
                flex_direction: FlexDirection::Column,
                gap: 0.0,
                ..Style::default()
            },
        );
        self.nodes.get_mut(&root).expect("table root exists").commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: total_width,
                height: total_height,
                radius: Radius::all(10.0),
                color: theme.border,
            },
            DrawCommand::RoundedRect {
                x: 1.0,
                y: 1.0,
                width: (total_width - 2.0).max(1.0),
                height: (total_height - 2.0).max(1.0),
                radius: Radius::all(9.0),
                color: theme.surface,
            },
        ];
        self.push_parent(root);
        let header = self.new_named_node(
            &format!("{id}:header"),
            Style {
                width: Some(total_width),
                height: Some(header_height),
                flex_direction: FlexDirection::Row,
                ..Style::default()
            },
        );
        self.push_parent(header);
        for (index, column) in columns.iter().enumerate() {
            let width = column_width(column);
            let key = format!("{id}:head:{index}");
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(width),
                    height: Some(header_height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            self.register_focusable(node, key.clone());
            let hovered = self.pointer_in_previous(&key);
            let focused = self.focused_id.as_deref() == Some(key.as_str());
            let sorted = state.sort_column == Some(index);
            let mut commands = vec![DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height: header_height,
                radius: Radius::all(3.0),
                color: if sorted {
                    mix_color(theme.surface, theme.accent, 0.12)
                } else if hovered {
                    mix_color(theme.surface, theme.foreground, 0.06)
                } else {
                    theme.surface
                },
            }];
            commands.push(DrawCommand::Text {
                x: theme.spacing.sm,
                y: theme.spacing.xs,
                text: column.title.to_string(),
                font_size: theme.font_sizes.body,
                italic: false,
                mono: false,
                family: None,
                weight: 600,
                color: if sorted {
                    theme.accent
                } else {
                    theme.foreground
                },
                max_width: Some((width - theme.spacing.sm * 2.0 - 10.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
            if sorted {
                let glyph = match state.sort_direction {
                    SortDirection::Ascending => "^",
                    SortDirection::Descending => "v",
                };
                commands.push(DrawCommand::Text {
                    x: width - 12.0,
                    y: theme.spacing.xs,
                    text: glyph.to_string(),
                    font_size: theme.font_sizes.small,
                    italic: false,
                    mono: false,
                    family: None,
                    weight: 400,
                    color: theme.accent,
                    max_width: Some(10.0),
                    alignment: TextAlignment::Start,
                });
            }
            if self.shows_focus_ring(focused) {
                commands.extend(focus_ring(0.0, 0.0, width, header_height, theme.accent));
            }
            self.register_access_node(
                node,
                Role::Button,
                column.title,
                Some(if sorted {
                    match state.sort_direction {
                        SortDirection::Ascending => "ascending",
                        SortDirection::Descending => "descending",
                    }
                } else {
                    "unsorted"
                }),
                focused,
            );
            self.nodes
                .get_mut(&node)
                .expect("table header cell exists")
                .commands = commands;
        }
        self.pop_parent();

        for (index, row) in rows.iter().enumerate() {
            let key = format!("{id}:row:{index}");
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(total_width),
                    height: Some(row_height),
                    flex_direction: FlexDirection::Row,
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            self.register_focusable(node, key.clone());
            let hovered = self.pointer_in_previous(&key);
            let focused = self.focused_id.as_deref() == Some(key.as_str());
            let selected = selected_id == row.id;
            let mut commands = Vec::new();
            if selected || hovered {
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: total_width,
                    height: row_height,
                    radius: Radius::all(6.0),
                    color: if selected {
                        theme.accent
                    } else {
                        mix_color(theme.surface, theme.foreground, 0.06)
                    },
                });
            }
            let label = row.cells.first().copied().unwrap_or(row.id);
            self.register_access_node(
                node,
                Role::Option,
                label,
                Some(if selected { "selected" } else { "unselected" }),
                focused,
            );
            self.push_parent(node);
            for (column_index, column) in columns.iter().enumerate() {
                let width = column_width(column);
                let cell_id = format!("{id}:cell:{index}:{column_index}");
                let cell = self.new_named_node(
                    &cell_id,
                    Style {
                        width: Some(width),
                        height: Some(row_height),
                        ..Style::default()
                    },
                );
                let text = row.cells.get(column_index).copied().unwrap_or("");
                let icon_name = (column_index == 0).then_some(row.icon).flatten();
                let icon_shift = if icon_name.is_some() { 18.0 } else { 0.0 };
                self.update_text(
                    cell,
                    text,
                    theme.font_sizes.body,
                    400,
                    if selected {
                        theme.on_accent
                    } else {
                        theme.muted
                    },
                    Some((width - theme.spacing.sm * 2.0 - icon_shift).max(1.0)),
                    TextAlignment::Start,
                    0.0,
                    false,
                    false,
                    None,
                );
                if let Some(icon_name) = icon_name {
                    let icon_color = if selected { theme.on_accent } else { theme.muted };
                    if let Some(icon) = Icon::builtin(icon_name) {
                        let icon_commands = vec![
                            DrawCommand::TransformPush {
                                transform: Transform {
                                    translation: Point::new(theme.spacing.sm, (row_height - 14.0) / 2.0),
                                    scale: Point::new(1.0, 1.0),
                                    rotation: 0.0,
                                },
                            },
                        ];
                        let mut icon = icon.to_commands_with_direction(14.0, icon_color, self.direction);
                        let mut all = icon_commands;
                        all.append(&mut icon);
                        all.push(DrawCommand::TransformPop);
                        if let Some(cell) = self.nodes.get_mut(&cell) {
                            cell.commands.splice(0..0, all);
                        }
                    }
                }
                if let Some(cell) = self.nodes.get_mut(&cell) {
                    for command in &mut cell.commands {
                        if let DrawCommand::Text { x, y, .. } = command {
                            *x = theme.spacing.sm + icon_shift;
                            *y = theme.spacing.xs;
                        }
                    }
                }
                
            }
            self.pop_parent();
            if self.shows_focus_ring(focused) {
                commands.extend(focus_ring(0.0, 0.0, total_width, row_height, theme.accent));
            }
            self.nodes
                .get_mut(&node)
                .expect("table row exists")
                .commands = commands;
        }
        self.pop_parent();
        self.register_access_node(root, Role::Group, id, None, false);
        TableResponse {
            selected_id,
            changed,
            clicked_id,
            sort,
        }
    }

    /// Creates a virtualized vertical list using the scroll-view state.
    pub fn list(
        &mut self,
        id: &str,
        state: &mut ListState,
        rows: &[ListRow<'_>],
        default_height: f32,
        viewport_height: f32,
    ) -> ListResponse {
        let theme = self.theme.clone();
        let default_height = if default_height.is_finite() && default_height > 0.0 {
            default_height
        } else {
            28.0
        };
        let viewport_height = viewport_height.max(1.0);
        let row_height = |row: &ListRow<'_>| {
            row.height
                .filter(|height| height.is_finite() && *height > 0.0)
                .unwrap_or(default_height)
        };
        let total_height = rows.iter().map(&row_height).sum::<f32>();
        let max_scroll = (total_height - viewport_height).max(0.0);
        let offset = match self.widget_states.get(id) {
            Some(WidgetState::ScrollView { offset, .. }) => *offset,
            _ => 0.0,
        }
        .clamp(0.0, max_scroll);
        let mut cursor = 0.0;
        let mut start = rows.len();
        let mut start_y = total_height;
        for (index, row) in rows.iter().enumerate() {
            let height = row_height(row);
            if cursor + height > offset {
                start = index;
                start_y = cursor;
                break;
            }
            cursor += height;
        }
        let mut end = start;
        let mut end_y = start_y;
        while end < rows.len() && end_y < offset + viewport_height {
            end_y += row_height(&rows[end]);
            end += 1;
        }
        let mut clicked_index = None;
        for index in start..end {
            let key = format!("{id}:list:row:{index}");
            if self.clicked_in_previous(&key) {
                clicked_index = Some((index, key));
                break;
            }
        }
        let previous_selected = state.selected_id.clone();
        let mut clicked_id = None;
        if let Some((index, key)) = &clicked_index {
            clicked_id = Some(rows[*index].id.to_string());
            state.selected_id.clear();
            state.selected_id.push_str(rows[*index].id);
            self.focused_id = Some(key.clone());
        }
        let changed = state.selected_id != previous_selected;
        let response_selected = state.selected_id.clone();

        let scroll = self.scroll_view(id, viewport_height, |context| {
            let width = context.current_content_width();
            if start_y > 0.0 {
                context.container().height(start_y);
            }
            for (index, row) in rows.iter().enumerate().take(end).skip(start) {
                let height = row_height(row);
                let key = format!("{id}:list:row:{index}");
                let node = context.new_named_node(
                    &key,
                    Style {
                        width: Some(width),
                        height: Some(height),
                        ..Style::default()
                    },
                );
                context.register_interaction(node, key.clone());
                context.register_focusable(node, key.clone());
                let hovered = context.pointer_in_previous(&key);
                let focused = context.focused_id.as_deref() == Some(key.as_str());
                let selected = response_selected == row.id;
                let mut commands = Vec::new();
                if selected || hovered {
                    commands.push(DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height,
                        radius: Radius::all(6.0),
                        color: if selected {
                            theme.accent
                        } else {
                            mix_color(theme.surface, theme.foreground, 0.06)
                        },
                    });
                }
                let mut text_x = theme.spacing.sm;
                if let Some(name) = row.icon {
                    if let Some(icon) = Icon::builtin(name) {
                        let icon_color = if selected { theme.on_accent } else { theme.muted };
                        commands.push(DrawCommand::TransformPush {
                            transform: Transform {
                                translation: Point::new(theme.spacing.sm, (height - 14.0) / 2.0),
                                scale: Point::new(1.0, 1.0),
                                rotation: 0.0,
                            },
                        });
                        commands.extend(icon.to_commands_with_direction(14.0, icon_color, context.direction));
                        commands.push(DrawCommand::TransformPop);
                        text_x = theme.spacing.sm + 14.0 + theme.spacing.sm;
                    }
                }
                let detail_width = row.detail.map(|detail| {
                    measure_text_width(detail, theme.font_sizes.small)
                }).unwrap_or(0.0);
                commands.push(DrawCommand::Text {
                    x: text_x,
                    y: theme.spacing.xs,
                    text: row.label.to_string(),
                    font_size: theme.font_sizes.body,
                    italic: false,
                    mono: false,
                    family: None,
                    weight: if selected { 600 } else { 400 },
                    color: if selected {
                        theme.on_accent
                    } else if hovered {
                        theme.foreground
                    } else {
                        theme.muted
                    },
                    max_width: Some((width - text_x - theme.spacing.sm - detail_width - theme.spacing.sm - 10.0).max(1.0)),
                    alignment: TextAlignment::Start,
                });
                if let Some(detail) = row.detail {
                    commands.push(DrawCommand::Text {
                        x: width - theme.spacing.sm - 10.0 - detail_width,
                        y: theme.spacing.xs,
                        text: detail.to_string(),
                        font_size: theme.font_sizes.small,
                        italic: false,
                        mono: false,
                        family: None,
                        weight: 400,
                        color: if selected { theme.on_accent } else { theme.muted },
                        max_width: Some(detail_width.max(1.0)),
                        alignment: TextAlignment::Start,
                    });
                }
                context.register_access_node(
                    node,
                    Role::Option,
                    row.label,
                    Some(if selected { "selected" } else { "unselected" }),
                    focused,
                );
                if let Some(node) = context.nodes.get_mut(&node) {
                    node.commands = commands;
                }
            }
            let remaining = (total_height - end_y).max(0.0);
            if remaining > 0.0 {
                context.container().height(remaining);
            }
        });
        ListResponse {
            selected_id: state.selected_id.clone(),
            changed,
            clicked_id,
            visible_start: start,
            visible_end: end,
            scroll: scroll.scroll.clamp(0.0, max_scroll),
            max_scroll,
        }
    }

    /// Creates a set of expandable accordion sections.
    pub fn accordion(
        &mut self,
        id: &str,
        state: &mut AccordionState,
        sections: &[AccordionSection<'_>],
    ) -> AccordionResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let font_size = theme.font_sizes.body;
        let header_height = measured_text_height(font_size) + theme.spacing.xs * 2.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.xs,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let mut toggled_id = None;
        let mut activated = None;
        for section in sections {
            let header_key = format!("{id}:header:{}", section.id);
            let clicked = self.clicked_in_previous(&header_key);
            let focused = self.focused_id.as_deref() == Some(header_key.as_str());
            let pressed = focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space));
            if clicked || pressed {
                activated = Some((header_key, section.id.to_string()));
                break;
            }
        }
        if let Some((header_key, section_id)) = activated {
            if state.is_open(&section_id) {
                state
                    .open
                    .retain(|open| open.as_str() != section_id.as_str());
            } else {
                if state.mode == super::accordion::AccordionMode::Single {
                    state.open.clear();
                }
                state.open.push(section_id.clone());
            }
            self.focused_id = Some(header_key);
            toggled_id = Some(section_id);
        }
        for section in sections {
            let header_key = format!("{id}:header:{}", section.id);
            let node = self.new_named_node(
                &header_key,
                Style {
                    width: Some(width),
                    height: Some(header_height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, header_key.clone());
            self.register_focusable(node, header_key.clone());
            let hovered = self.pointer_in_previous(&header_key);
            let focused = self.focused_id.as_deref() == Some(header_key.as_str());
            let open = state.is_open(section.id);
            let mut commands = vec![DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height: header_height,
                radius: Radius::all(4.0),
                color: if open {
                    mix_color(theme.surface, theme.accent, 0.12)
                } else if hovered {
                    mix_color(theme.surface, theme.foreground, 0.06)
                } else {
                    theme.surface
                },
            }];
            let center = header_height / 2.0;
            let arrow = if open {
                [
                    (8.0, center - 3.0, 12.0, center + 1.0),
                    (12.0, center + 1.0, 16.0, center - 3.0),
                ]
            } else {
                [
                    (9.0, center - 4.0, 13.0, center),
                    (13.0, center, 9.0, center + 4.0),
                ]
            };
            for (x1, y1, x2, y2) in arrow {
                commands.push(DrawCommand::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width: 2.0,
                    color: theme.muted,
                });
            }
            commands.push(DrawCommand::Text {
                x: 24.0,
                y: theme.spacing.xs,
                text: section.title.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: if open { 600 } else { 400 },
                color: theme.foreground,
                max_width: Some((width - 32.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
            if self.shows_focus_ring(focused) {
                commands.extend(focus_ring(0.0, 0.0, width, header_height, theme.accent));
            }
            self.register_access_node(
                node,
                Role::Disclosure,
                section.title,
                Some(if open { "expanded" } else { "collapsed" }),
                focused,
            );
            self.nodes
                .get_mut(&node)
                .expect("accordion header exists")
                .commands = commands;
            if open {
                self.text(section.content)
                    .font_size(theme.font_sizes.small)
                    .color(theme.muted)
                    .max_width(width);
            }
        }
        self.pop_parent();
        self.register_access_node(root, Role::Group, id, None, false);
        let changed = toggled_id.is_some();
        AccordionResponse {
            toggled_id,
            changed,
        }
    }

    /// Starts MyGo's `Accordion`: a column of sections in one bordered,
    /// clipped box, each built with [`Self::accordion_item`]
    /// (`ui/collapsible.go` coreAccordion).
    pub fn accordion_group(&mut self, id: &str, build: impl FnOnce(&mut Context)) {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
        );
        // MyGo's border takes layout space and its radius clips the
        // sections' own backgrounds to the box.
        // MyGo's border takes layout space — `padX() = pad + border` — so
        // the box around the sections is inset by the border it draws
        // (`ui/layout.go`, `ui/collapsible.go` coreAccordion).
        self.set_node_fill(root, Color::rgba(0, 0, 0, 0), 6.0);
        self.set_node_border(root, theme.border, 6.0);
        self.set_node_clip(root, true);
        if let Some(node) = self.nodes.get_mut(&root) {
            node.style.padding = Edges::all(1.0);
        }
        self.push_parent(root);
        build(self);
        self.pop_parent();
    }

    /// Adds one section to an [`Self::accordion_group`]: a header showing
    /// `title` that a click, Enter or Space opens and closes, and below it
    /// what `content` builds while it is open (`ui/collapsible.go`
    /// coreAccordionItem).
    pub fn accordion_item(
        &mut self,
        id: &str,
        title: &str,
        open: &mut bool,
        content: impl FnOnce(&mut Context),
    ) -> CollapsibleState {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let font_size = theme.font_sizes.body;
        let space = theme.spacing.base;
        let item = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
        );
        // Every section but the first draws its top border, as MyGo's
        // `BorderWidth(1, 0, 0, 0)` does. The parent already holds this
        // node, so a single child means this is the first.
        let first = self
            .parent_stack
            .last()
            .and_then(|parent| self.nodes.get(parent))
            .is_none_or(|parent| parent.children.len() <= 1);
        if !first {
            self.set_node_top_border(item, theme.border);
            // MyGo's border takes layout space, so the line pushes the
            // section down by the pixel it occupies (`ui/layout.go`).
            if let Some(node) = self.nodes.get_mut(&item) {
                node.style.padding.top = 1.0;
            }
        }
        self.push_parent(item);
        let header_key = format!("{id}:header");
        // MyGo's header is a row that its padding sizes: Space(2.5) above
        // and below the title, Space(3) at the ends, and Space(2) between
        // the title and the arrow (`ui/collapsible.go` coreAccordionItem).
        let header = self.new_named_node(
            &header_key,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                gap: space * 2.0,
                padding: Edges::symmetric(space * 2.5, space * 3.0),
                ..Style::default()
            },
        );
        self.register_interaction(header, header_key.clone());
        self.register_focusable(header, header_key.clone());
        let hovered = self.pointer_in_previous(&header_key);
        if hovered && self.input.pointer_down {
            self.focused_id = Some(header_key.clone());
        }
        let focused = self.focused_id.as_deref() == Some(header_key.as_str());
        let toggled = self.clicked_in_previous(&header_key)
            || (focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space)));
        if toggled {
            *open = !*open;
        }
        if hovered {
            self.set_node_fill(header, theme.surface_hover, 0.0);
        }
        self.register_access_node(
            header,
            Role::Disclosure,
            title,
            Some(if *open { "expanded" } else { "collapsed" }),
            focused,
        );
        // The title grows and the arrow keeps its Space(4) box, as MyGo's
        // header row does.
        self.push_parent(header);
        // MyGo's `coreText(c, title).Grow(1)`: the title takes the row
        // left over, which puts the arrow at the header's far edge.
        let title_width = (width - space * 6.0 - space * 4.0 - space * 2.0).max(1.0);
        self.text(title)
            .weight(500)
            .color(theme.foreground)
            .width(title_width)
            .max_width(title_width);
        let arrow_size = space * 4.0;
        let arrow = self.new_named_node(
            &format!("{id}:arrow"),
            Style {
                width: Some(arrow_size),
                height: Some(arrow_size),
                ..Style::default()
            },
        );
        let (cx, cy, d) = (arrow_size / 2.0, arrow_size / 2.0, arrow_size / 8.0);
        // MyGo draws the arrow from `90 + 180*progress`: pointing down
        // while closed and up while open (`ui/collapsible.go`).
        let angle: f32 = if *open { 270.0_f32.to_radians() } else { 90.0_f32.to_radians() };
        let rotate = |x: f32, y: f32| {
            let (sin, cos) = angle.sin_cos();
            Point::new(cx + x * cos - y * sin, cy + x * sin + y * cos)
        };
        let start = rotate(-d, -2.0 * d);
        let middle = rotate(d, 0.0);
        let end = rotate(-d, 2.0 * d);
        self.set_node_commands(
            arrow,
            vec![DrawCommand::SvgPath {
                x: 0.0,
                y: 0.0,
                width: arrow_size,
                height: arrow_size,
                path: format!(
                    "M{} {}L{} {}L{} {}",
                    start.x, start.y, middle.x, middle.y, end.x, end.y
                ),
                fill: None,
                stroke: Some(theme.muted),
                stroke_width: 1.5,
            }],
        );
        self.pop_parent();
        if *open {
            let panel = self.new_named_node(
                &format!("{id}:panel"),
                Style {
                    width: Some(width),
                    flex_direction: FlexDirection::Column,
                    gap: space * 2.0,
                    padding: Edges::new(0.0, space * 3.0, space * 3.0, space * 3.0),
                    ..Style::default()
                },
            );
            self.push_parent(panel);
            content(self);
            self.pop_parent();
        }
        self.pop_parent();
        CollapsibleState {
            changed: toggled,
            open: *open,
            focused,
            hovered,
        }
    }

    /// Creates a row of clickable breadcrumb links.
    pub fn breadcrumbs(&mut self, id: &str, items: &[BreadcrumbItem<'_>]) -> BreadcrumbsResponse {
        // MyGo's Breadcrumbs (`ui/feedback.go` coreBreadcrumbs): a row of
        // Space(0.5) = 2 gaps. Each link is a row padded Space(0.5) = 2 down
        // and Space(1.5) = 6 across, radius 6, at least Space(6) = 24 wide,
        // its text in the muted color; the last item, where the path is, is
        // plain text at weight 500 in the text color. Between items a
        // Space(3) = 12 box holds a chevron stroked 1.2 in the muted color.
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let text_height = measured_text_height(font_size);
        let gap = theme.spacing.base * 0.5;
        let item_pad_v = theme.spacing.base * 0.5;
        let item_pad_h = theme.spacing.base * 1.5;
        let item_height = text_height + item_pad_v * 2.0;
        // A row shrinks to its items, as MyGo's `coreRow` does without Grow
        // or Fill, so the preview centers the path rather than stretching it.
        // Its gap falls between every pair of children: a separator sits
        // between two items, so each costs a gap on either side.
        let separator = theme.spacing.base * 3.0;
        let mut content_width = 0.0;
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                content_width += gap + separator + gap;
            }
            // Every item is the same row, the last one only losing its link
            // behavior and taking weight 500, which measures wider.
            content_width += crumb_width(item.label, font_size, index + 1 == items.len(), item_pad_h);
        }
        let root = self.new_named_node(
            id,
            Style {
                width: Some(content_width),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                gap,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let mut clicked = None;
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                // A chevron points right, along the path: a Space(3) box with
                // a 1.2-stroked path from its left quarter to its right.
                let separator = theme.spacing.base * 3.0;
                let node = self.new_named_node(
                    &format!("{id}:separator:{index}"),
                    Style {
                        width: Some(separator),
                        height: Some(separator),
                        ..Style::default()
                    },
                );
                let cx = separator / 2.0;
                let cy = separator / 2.0;
                let d = separator / 6.0;
                self.nodes
                    .get_mut(&node)
                    .expect("breadcrumb separator exists")
                    .commands = vec![DrawCommand::SvgPath {
                    x: 0.0,
                    y: 0.0,
                    width: separator,
                    height: separator,
                    path: format!(
                        "M{} {}L{} {}L{} {}",
                        cx - d,
                        cy - 2.0 * d,
                        cx + d,
                        cy,
                        cx - d,
                        cy + 2.0 * d
                    ),
                    fill: None,
                    stroke: Some(theme.muted),
                    stroke_width: 1.2,
                }];
            }
            let last = index + 1 == items.len();
            let key = format!("{id}:crumb:{index}");
            // A link is at least Space(6) = 24 across, its text padded
            // Space(1.5) = 6 either side; the last item, where the path is,
            // keeps the padding but takes no minimum.
            let item_width = crumb_width(item.label, font_size, last, item_pad_h);
            if last {
                // Where the path is: no link, but the current page.
                let node = self.new_named_node(
                    &key,
                    Style {
                        width: Some(item_width),
                        height: Some(item_height),
                        ..Style::default()
                    },
                );
                self.nodes
                    .get_mut(&node)
                    .expect("breadcrumb current page exists")
                    .commands = vec![DrawCommand::Text {
                    x: item_pad_h,
                    y: item_pad_v,
                    text: item.label.to_string(),
                    font_size,
                    italic: false,
                    mono: false,
                    family: None,
                    weight: 500,
                    color: theme.foreground,
                    max_width: Some((item_width - item_pad_h * 2.0).max(1.0)),
                    alignment: TextAlignment::Start,
                }];
                self.register_access_node(node, Role::Label, item.label, Some("current"), false);
                continue;
            }
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(item_width),
                    height: Some(item_height),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            self.register_focusable(node, key.clone());
            let hovered = self.pointer_in_previous(&key);
            let focused = self.focused_id.as_deref() == Some(key.as_str());
            let activated = self.clicked_in_previous(&key)
                || (focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space)));
            if activated {
                clicked = Some(item.id.to_string());
                self.focused_id = Some(key);
            }
            let mut commands = Vec::new();
            if hovered {
                // A hovered link fills with the hovered surface.
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: item_width,
                    height: item_height,
                    radius: Radius::all(6.0),
                    color: theme.surface_hover,
                });
            }
            commands.push(DrawCommand::Text {
                x: item_pad_h,
                y: item_pad_v,
                text: item.label.to_string(),
                font_size,
                italic: false,
                mono: false,
                family: None,
                weight: 400,
                color: theme.muted,
                max_width: Some((item_width - item_pad_h * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
            if self.shows_focus_ring(focused) {
                commands.extend(focus_ring(0.0, 0.0, item_width, item_height, theme.accent));
            }
            self.register_access_node(node, Role::Button, item.label, Some(item.id), focused);
            self.nodes
                .get_mut(&node)
                .expect("breadcrumb item exists")
                .commands = commands;
        }
        self.pop_parent();
        self.register_access_node(root, Role::Group, id, None, false);
        BreadcrumbsResponse { clicked }
    }

    /// Creates a grouped single-selection sidebar.
    pub fn sidebar(
        &mut self,
        id: &str,
        selected: &mut String,
        sections: &[SidebarSection<'_>],
    ) -> SidebarResponse {
        // MyGo's Sidebar (`ui/sidebar.go` coreSidebar, coreSidebarItem):
        // a scroll container padded Space(2.5) = 10 with Space(0.5) = 2
        // between its sections, on the surface. Each section heads with a
        // Space(1)/Space(2)-padded row whose title is FontSize - 1 at weight
        // 600 in the muted color, and spaces Space(3) = 12 above itself when
        // another section precedes it. An item is a row padded Space(1.5) =
        // 6 vertically and Space(2) = 8 across, gap Space(2) = 8, radius 6:
        // its icon 16px, its label taking the rest on one line, and the
        // chosen item filling with the pressed surface (the accent while the
        // sidebar has the focus).
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let font_size = theme.font_sizes.body;
        let padding = theme.spacing.base * 2.5;
        let item_gap = theme.spacing.base * 2.0;
        let item_padding_v = theme.spacing.base * 1.5;
        let item_padding_h = theme.spacing.base * 2.0;
        let item_height = measured_text_height(font_size) + item_padding_v * 2.0;
        let icon_size = 16.0_f32;
        let mut selected_index: Option<usize> = None;
        let mut first_id = None;
        let mut total: usize = 0;
        for section in sections {
            for item in section.items {
                if first_id.is_none() {
                    first_id = Some(item.id);
                }
                if selected_index.is_none() && item.id == selected.as_str() {
                    selected_index = Some(total);
                }
                total += 1;
            }
        }
        if total == 0 {
            selected.clear();
        } else if selected_index.is_none() {
            if let Some(first) = first_id {
                selected.clear();
                selected.push_str(first);
                selected_index = Some(0);
            }
        }
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.base * 0.5,
                padding: Edges::all(padding),
                ..Style::default()
            },
        );
        self.register_interaction(root, id.to_string());
        self.register_focusable(root, id.to_string());
        let root_hovered = self.pointer_in_previous(id);
        if root_hovered && self.input.pointer_down {
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        let mut changed = false;
        if focused && total > 0 {
            let step = if self.key_pressed(Key::Home) {
                Some(0_usize)
            } else if self.key_pressed(Key::End) {
                Some(total - 1)
            } else if self.key_pressed(Key::ArrowUp) || self.key_pressed(Key::ArrowLeft) {
                selected_index.map(|index| index.saturating_sub(1))
            } else if self.key_pressed(Key::ArrowDown) || self.key_pressed(Key::ArrowRight) {
                selected_index.map(|index| (index + 1).min(total - 1))
            } else {
                None
            };
            if let Some(next) = step {
                if selected_index != Some(next) {
                    selected_index = Some(next);
                    if let Some(next_id) = sidebar_item_id(sections, next) {
                        selected.clear();
                        selected.push_str(next_id);
                        changed = true;
                    }
                }
            }
        }
        let mut hovered = root_hovered;
        let mut clicked_id = None;
        let inner_width = (width - padding * 2.0).max(1.0);
        self.push_parent(root);
        let mut flat_index = 0;
        for (section_index, section) in sections.iter().enumerate() {
            let mut head = self.new_node(Style {
                width: Some(inner_width),
                height: Some(measured_text_height(font_size - 1.0) + theme.spacing.base * 2.0),
                padding: Edges::new(
                    theme.spacing.base,
                    theme.spacing.base * 2.0,
                    theme.spacing.base,
                    theme.spacing.base * 2.0,
                ),
                align_items: AlignItems::Center,
                ..Style::default()
            });
            if section_index > 0 {
                // Another section above spaces Space(3) over this one.
                let node = self.nodes.get_mut(&head).expect("sidebar head exists");
                node.style.margin.top = theme.spacing.base * 3.0;
            }
            self.nodes.get_mut(&head).expect("sidebar head exists").commands = vec![
                DrawCommand::Text {
                    x: theme.spacing.base * 2.0,
                    y: theme.spacing.base,
                    text: section.title.to_string(),
                    font_size: font_size - 1.0,
                    italic: false,
                    mono: false,
                    family: None,
                    weight: 600,
                    color: theme.muted,
                    max_width: Some(inner_width - theme.spacing.base * 4.0),
                    alignment: TextAlignment::Start,
                },
            ];
            let _ = &mut head;
            for item in section.items {
                let key = format!("{id}:item:{flat_index}");
                let node = self.new_named_node(
                    &key,
                    Style {
                        width: Some(inner_width),
                        height: Some(item_height),
                        padding: Edges::new(
                            item_padding_v,
                            item_padding_h,
                            item_padding_v,
                            item_padding_h,
                        ),
                        gap: item_gap,
                        align_items: AlignItems::Center,
                        ..Style::default()
                    },
                );
                self.register_interaction(node, key.clone());
                let item_hovered = self.pointer_in_previous(&key);
                if item_hovered {
                    hovered = true;
                }
                if item_hovered && self.input.pointer_down {
                    self.focused_id = Some(id.to_string());
                }
                if self.clicked_in_previous(&key) {
                    clicked_id = Some(item.id.to_string());
                }
                let item_selected = selected_index == Some(flat_index);
                let item_color = if item_selected && focused {
                    theme.on_accent
                } else {
                    theme.foreground
                };
                let mut commands = Vec::new();
                if item_selected {
                    commands.push(DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: inner_width,
                        height: item_height,
                        radius: Radius::all(6.0),
                        color: if focused {
                            theme.accent
                        } else {
                            theme.surface_pressed
                        },
                    });
                } else if item_hovered {
                    commands.push(DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: inner_width,
                        height: item_height,
                        radius: Radius::all(6.0),
                        color: theme.surface_hover,
                    });
                }
                let icon_color = if item_selected && focused {
                    theme.on_accent
                } else {
                    theme.accent
                };
                let mut text_x = item_padding_h;
                if let Some(name) = item.icon {
                    if let Some(icon) = Icon::builtin(name) {
                        let icon_y = item_padding_v + (measured_text_height(font_size) - icon_size) / 2.0;
                        commands.push(DrawCommand::TransformPush {
                            transform: Transform {
                                translation: Point::new(item_padding_h, icon_y),
                                scale: Point::new(1.0, 1.0),
                                rotation: 0.0,
                            },
                        });
                        commands.extend(icon.to_commands_with_direction(icon_size, icon_color, self.direction));
                        commands.push(DrawCommand::TransformPop);
                        text_x = item_padding_h + icon_size + item_gap;
                    }
                }
                let badge_width = item
                    .badge
                    .map(|badge| {
                        measure_text_width(badge, font_size - 2.0) + theme.spacing.base * 3.0
                    })
                    .unwrap_or(0.0);
                let text_width =
                    (inner_width - text_x - item_padding_h - badge_width).max(1.0);
                commands.push(DrawCommand::Text {
                    x: text_x,
                    y: item_padding_v,
                    text: item.label.to_string(),
                    font_size,
                    italic: false,
                    mono: false,
                    family: None,
                    weight: 400,
                    color: item_color,
                    max_width: Some(text_width),
                    alignment: TextAlignment::Start,
                });
                if let Some(badge) = item.badge {
                    // MyGo's badge (`ui/sidebar.go` coreBadge): Space(1.5) of
                    // side padding around a Space(6) pill in the text color at
                    // a tenth of an alpha, unless the caller fills it with
                    // the accent instead.
                    let badge_height = measured_text_height(font_size - 2.0);
                    let badge_x = inner_width - badge_width - item_padding_h;
                    let badge_y = item_padding_v + (measured_text_height(font_size) - badge_height) / 2.0;
                    let (badge_fill, badge_text) = if item.badge_accent {
                        (theme.accent, theme.on_accent)
                    } else {
                        (
                            Color::rgba(
                                theme.foreground.r,
                                theme.foreground.g,
                                theme.foreground.b,
                                26,
                            ),
                            theme.foreground,
                        )
                    };
                    commands.push(DrawCommand::RoundedRect {
                        x: badge_x,
                        y: badge_y,
                        width: badge_width,
                        height: badge_height,
                        radius: Radius::all(999.0),
                        color: badge_fill,
                    });
                    commands.push(DrawCommand::Text {
                        x: badge_x + theme.spacing.base * 1.5,
                        y: badge_y,
                        text: badge.to_string(),
                        font_size: font_size - 2.0,
                        italic: false,
                        mono: false,
                        family: None,
                        weight: 400,
                        color: badge_text,
                        max_width: Some(badge_width),
                        alignment: TextAlignment::Start,
                    });
                }
                self.register_access_node(
                    node,
                    Role::Option,
                    item.label,
                    Some(if item_selected {
                        "selected"
                    } else {
                        "unselected"
                    }),
                    false,
                );
                self.nodes
                    .get_mut(&node)
                    .expect("sidebar item exists")
                    .commands = commands;
                flat_index += 1;
            }
        }
        self.pop_parent();
        let mut selected_id = selected.clone();
        if let Some(index) = selected_index {
            if let Some(id) = sidebar_item_id(sections, index) {
                selected_id.clear();
                selected_id.push_str(id);
            }
        }
        if let Some(clicked) = &clicked_id {
            if selected_id != *clicked {
                selected_id.clone_from(clicked);
                selected.clone_from(&selected_id);
                changed = true;
            }
            self.focused_id = Some(id.to_string());
        }
        let focused = self.focused_id.as_deref() == Some(id);
        self.register_access_node(root, Role::Group, id, Some(&selected_id), focused);
        SidebarResponse {
            selected_id,
            changed,
            clicked_id,
            focused,
            hovered,
        }
    }

    /// Creates a two-panel split view with a draggable divider.
    pub fn split_view(
        &mut self,
        id: &str,
        state: &mut SplitViewState,
        orientation: SplitOrientation,
        first: impl FnOnce(&mut Context),
        second: impl FnOnce(&mut Context),
    ) -> SplitViewResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let height = self.current_content_height();
        let mut ratio = if state.ratio.is_finite() {
            state.ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO)
        } else {
            0.5
        };
        let thickness = theme.spacing.xs.max(6.0);
        let divider_key = format!("{id}:divider");
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        self.register_interaction(root, id.to_string());
        let mut dragging = match self.widget_states.get(id) {
            Some(WidgetState::SplitView { dragging }) => *dragging,
            _ => false,
        };
        let hovered = self.pointer_in_previous(&divider_key);
        let pointer = self.local_pointer_in_previous(id);
        let root_bounds = self.previous_regions.get(id).map(|region| region.bounds);
        let usable = match orientation {
            SplitOrientation::Horizontal => (width - thickness).max(1.0),
            SplitOrientation::Vertical => (height - thickness).max(1.0),
        };
        let mut changed = false;
        if hovered && self.input.pointer_down {
            dragging = true;
            self.focused_id = Some(divider_key.clone());
        }
        if dragging {
            if let (Some(point), Some(bounds)) = (pointer, root_bounds) {
                let next = match orientation {
                    SplitOrientation::Horizontal => (point.x - bounds.x - thickness / 2.0) / usable,
                    SplitOrientation::Vertical => (point.y - bounds.y - thickness / 2.0) / usable,
                };
                let next = next.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO);
                if (next - ratio).abs() > f32::EPSILON {
                    ratio = next;
                    changed = true;
                }
            }
            if !self.input.pointer_down && self.input.released.is_some() {
                dragging = false;
            }
        }
        let focused = self.focused_id.as_deref() == Some(divider_key.as_str());
        if focused {
            let step = if self.key_pressed(Key::ArrowLeft) || self.key_pressed(Key::ArrowUp) {
                Some(-0.02_f32)
            } else if self.key_pressed(Key::ArrowRight) || self.key_pressed(Key::ArrowDown) {
                Some(0.02_f32)
            } else {
                None
            };
            if let Some(step) = step {
                let next = (ratio + step).clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO);
                if (next - ratio).abs() > f32::EPSILON {
                    ratio = next;
                    changed = true;
                }
            }
        }
        state.ratio = ratio;
        self.widget_states
            .insert(id.to_string(), WidgetState::SplitView { dragging });

        let first_size = usable * ratio;
        let (first_width, first_height, second_width, second_height, divider_x, divider_y) =
            match orientation {
                SplitOrientation::Horizontal => (
                    first_size,
                    height,
                    usable - first_size,
                    height,
                    first_size,
                    0.0,
                ),
                SplitOrientation::Vertical => (
                    width,
                    first_size,
                    width,
                    usable - first_size,
                    0.0,
                    first_size,
                ),
            };
        let divider_width = match orientation {
            SplitOrientation::Horizontal => thickness,
            SplitOrientation::Vertical => width,
        };
        let divider_height = match orientation {
            SplitOrientation::Horizontal => height,
            SplitOrientation::Vertical => thickness,
        };
        let divider = self.new_named_node(
            &divider_key,
            Style {
                width: Some(divider_width),
                height: Some(divider_height),
                position: Position::Absolute,
                left: Some(divider_x),
                top: Some(divider_y),
                ..Style::default()
            },
        );
        self.register_interaction(divider, divider_key.clone());
        self.register_focusable(divider, divider_key.clone());
        let divider_color = if dragging || hovered {
            theme.accent
        } else {
            mix_color(theme.surface, theme.border, 0.6)
        };
        let mut divider_commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width: divider_width,
            height: divider_height,
            radius: Radius::all(3.0),
            color: divider_color,
        }];
        let grip_x = divider_width / 2.0;
        let grip_y = divider_height / 2.0;
        for offset in [-4.0_f32, 0.0, 4.0] {
            let (x, y) = match orientation {
                SplitOrientation::Horizontal => (grip_x, grip_y + offset),
                SplitOrientation::Vertical => (grip_x + offset, grip_y),
            };
            divider_commands.push(DrawCommand::Circle {
                x,
                y,
                radius: 1.2,
                color: theme.on_accent,
            });
        }
        if self.shows_focus_ring(focused) {
            divider_commands.extend(focus_ring(
                0.0,
                0.0,
                divider_width,
                divider_height,
                theme.accent,
            ));
        }
        self.register_access_node(
            divider,
            Role::Slider,
            "split view",
            Some(format!("{:.0}%", ratio * 100.0).as_str()),
            focused,
        );
        self.nodes
            .get_mut(&divider)
            .expect("split divider exists")
            .commands = divider_commands;

        let first_panel = self.new_named_node(
            &format!("{id}:first"),
            Style {
                width: Some(first_width),
                height: Some(first_height),
                position: Position::Absolute,
                left: Some(0.0),
                top: Some(0.0),
                ..Style::default()
            },
        );
        self.push_parent(first_panel);
        first(self);
        self.pop_parent();
        let second_panel = self.new_named_node(
            &format!("{id}:second"),
            Style {
                width: Some(second_width),
                height: Some(second_height),
                position: Position::Absolute,
                left: Some(match orientation {
                    SplitOrientation::Horizontal => first_size + thickness,
                    SplitOrientation::Vertical => 0.0,
                }),
                top: Some(match orientation {
                    SplitOrientation::Horizontal => 0.0,
                    SplitOrientation::Vertical => first_size + thickness,
                }),
                ..Style::default()
            },
        );
        self.push_parent(second_panel);
        second(self);
        self.pop_parent();
        let divider_bounds = self
            .previous_regions
            .get(&divider_key)
            .map(|region| region.bounds);
        SplitViewResponse {
            ratio,
            changed,
            dragging,
            focused,
            hovered,
            divider_bounds,
        }
    }

    /// Creates a split whose first pane keeps a DIP size, as MyGo's
    /// `ui.Split` does (`ui/split.go`).
    ///
    /// The panes meet at a one-pixel line; the handle the pointer drags
    /// spans `grip` DIPs over either pane. Dragging moves the divider from
    /// the size it shows, the panes staying at least `min_pane` wide.
    pub fn split_sized(
        &mut self,
        id: &str,
        size: &mut f32,
        orientation: SplitOrientation,
        min_pane: f32,
        grip: f32,
        first: impl FnOnce(&mut Context),
        second: impl FnOnce(&mut Context),
    ) -> SplitViewResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let height = self.current_content_height();
        let vertical = orientation == SplitOrientation::Vertical;
        let total = if vertical { height } else { width };
        let (handle_key, divider_key) = (format!("{id}:handle"), format!("{id}:divider"));
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                flex_direction: if vertical {
                    FlexDirection::Column
                } else {
                    FlexDirection::Row
                },
                align_items: AlignItems::Stretch,
                ..Style::default()
            },
        );
        self.register_interaction(root, id.to_string());
        self.push_parent(root);

        let (mut dragging, mut shown) = match self.widget_states.get(id) {
            Some(WidgetState::SplitSized { dragging, shown }) => (*dragging, *shown),
            _ => (false, *size),
        };
        // A size the room cannot show still stands: the pane takes it back
        // when there is room again (`ui/split.go` fitSplit).
        let fit = |value: f32| -> f32 {
            let max = (total - 1.0 - min_pane).max(min_pane);
            value.clamp(min_pane, max)
        };
        shown = if *size > min_pane { fit(*size) } else { shown };

        let hovered = self.pointer_in_previous(&handle_key);
        let pointer = self.local_pointer_in_previous(id);
        let root_bounds = self.previous_regions.get(id).map(|region| region.bounds);
        let mut changed = false;
        if hovered && self.input.pointer_down {
            dragging = true;
            self.focused_id = Some(divider_key.clone());
        }
        if dragging {
            if let Some(point) = pointer {
                let offset = root_bounds
                    .map(|bounds| {
                        if vertical {
                            point.y - bounds.y
                        } else {
                            point.x - bounds.x
                        }
                    })
                    .unwrap_or(shown);
                let next = fit(offset - 0.5);
                if (next - shown).abs() > f32::EPSILON {
                    shown = next;
                    *size = next;
                    changed = true;
                }
            }
            if !self.input.pointer_down && self.input.released.is_some() {
                dragging = false;
            }
        }
        let focused = self.focused_id.as_deref() == Some(divider_key.as_str());
        if focused {
            let step = if self.key_pressed(Key::ArrowLeft) || self.key_pressed(Key::ArrowUp) {
                Some(-theme.spacing.base * 2.5)
            } else if self.key_pressed(Key::ArrowRight) || self.key_pressed(Key::ArrowDown) {
                Some(theme.spacing.base * 2.5)
            } else {
                None
            };
            if let Some(step) = step {
                let next = fit(shown + step);
                if (next - shown).abs() > f32::EPSILON {
                    shown = next;
                    *size = next;
                    changed = true;
                }
            }
        }
        self.widget_states
            .insert(id.to_string(), WidgetState::SplitSized { dragging, shown });

        // The panes are the split's own flex children: first keeps the DIP
        // size, the one-pixel line follows, and second takes the rest.
        let first_panel = self.new_named_node(
            &format!("{id}:first"),
            Style {
                width: Some(if vertical { width } else { shown }),
                height: Some(if vertical { shown } else { height }),
                min_width: (!vertical).then_some(shown.min(min_pane)).unwrap_or(0.0),
                min_height: vertical.then_some(shown.min(min_pane)).unwrap_or(0.0),
                ..Style::default()
            },
        );
        let _ = &first_panel;
        self.push_parent(first_panel);
        first(self);
        self.pop_parent();

        let divider = self.new_named_node(
            &divider_key,
            Style {
                width: (!vertical).then_some(1.0),
                height: vertical.then_some(1.0),
                ..Style::default()
            },
        );
        self.register_interaction(divider, divider_key.clone());
        self.register_focusable(divider, divider_key.clone());
        let line_color = if dragging || hovered {
            theme.accent
        } else {
            theme.border
        };
        self.nodes.get_mut(&divider).expect("split divider exists").fill =
            Some((line_color, Radius::all(0.0)));

        let second_panel = self.new_named_node(
            &format!("{id}:second"),
            Style {
                flex_grow: 1.0,
                // The cross axis is the split's whole box: MyGo's panes
                // stretch to it, and a nested split reads its own room from
                // the box it is given.
                width: Some(if vertical { width } else { (total - shown - 1.0).max(min_pane) }),
                height: Some(if vertical { (total - shown - 1.0).max(min_pane) } else { height }),
                min_width: (!vertical).then_some(min_pane).unwrap_or(0.0),
                min_height: vertical.then_some(min_pane).unwrap_or(0.0),
                ..Style::default()
            },
        );
        let _ = &second_panel;
        self.push_parent(second_panel);
        second(self);
        self.pop_parent();

        // The handle spans the grip over the line, placed on the divider
        // as the last frame laid it out (`ui/split.go` placeOn).
        let divider_bounds = self
            .previous_regions
            .get(&divider_key)
            .map(|region| region.bounds);
        let handle = self.new_named_node(
            &handle_key,
            Style {
                width: Some(if vertical { width } else { grip }),
                height: Some(if vertical { grip } else { height }),
                position: Position::Absolute,
                left: Some(if vertical {
                    0.0
                } else {
                    divider_bounds
                        .map(|bounds| bounds.x - root_bounds.map(|r| r.x).unwrap_or(0.0))
                        .unwrap_or(shown)
                        - (grip - 1.0) / 2.0
                }),
                top: Some(if vertical {
                    divider_bounds
                        .map(|bounds| bounds.y - root_bounds.map(|r| r.y).unwrap_or(0.0))
                        .unwrap_or(shown)
                        - (grip - 1.0) / 2.0
                } else {
                    0.0
                }),
                ..Style::default()
            },
        );
        self.register_interaction(handle, handle_key.clone());
        if focused {
            let ring = focus_ring(
                0.0,
                0.0,
                if vertical { width } else { grip },
                if vertical { grip } else { height },
                theme.accent,
            );
            self.nodes.get_mut(&handle).expect("split handle exists").commands = ring.to_vec();
        }

        self.register_access_node(divider, Role::Slider, "split divider", None, focused);
        self.pop_parent();
        SplitViewResponse {
            ratio: if total > 1.0 { shown / total } else { 0.0 },
            changed,
            dragging,
            focused,
            hovered,
            divider_bounds,
        }
    }

    /// Lays out a labeled form field with description and error text.
    /// Starts a form: a column of fields whose labels line up in one
    /// column as wide as the widest, as MyGo's `Form` does
    /// (`ui/form.go` coreForm, alignLabels).
    pub fn form_group(&mut self, id: &str, build: impl FnOnce(&mut Context)) {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.base * 3.0,
                ..Style::default()
            },
        );
        // The label column is as wide as the widest label the fields hold,
        // which the demo measures before the form is built.
        let label_width = self.form_label_width.unwrap_or(0.0);
        self.form_label_width = Some(label_width);
        self.push_parent(root);
        build(self);
        self.pop_parent();
        self.form_label_width = None;
    }

    /// Sets the width of the label column [`Self::form_group`] lays out.
    pub fn set_form_label_width(&mut self, width: f32) {
        self.form_label_width = Some(width);
    }

    /// Creates a field inside a [`Self::form_group`]: the label in a column
    /// as wide as the form's widest, and the control filling the rest of
    /// the row, as MyGo's `Field` does in a `Form` (`ui/form.go`).
    pub fn form_row(
        &mut self,
        id: &str,
        label: &str,
        description: Option<&str>,
        error: Option<&str>,
        content: impl FnOnce(&mut Context),
    ) -> FormFieldState {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let has_error = error.is_some();
        let label_width = self.form_label_width.unwrap_or(0.0);
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::FlexStart,
                gap: theme.spacing.base * 3.0,
                // MyGo lines a field's label up on the baseline of the
                // control's first line, not on the row's top (`ui/form.go`
                // coreField sets `f.baselines = true`).
                baselines: true,
                ..Style::default()
            },
        );
        self.push_parent(root);
        // The label sits at the end of its column, beside the control's
        // first line.
        let label_box = self.new_named_node(
            &format!("{id}:label"),
            Style {
                width: Some(label_width.max(1.0)),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::FlexEnd,
                ..Style::default()
            },
        );
        self.push_parent(label_box);
        // MyGo's field label keeps the theme's text color even while the
        // field is invalid; only the message below turns (`ui/form.go`).
        // It wraps to no width of its own: the column it sits in is as wide
        // as the widest label (`ui/form.go` alignLabels).
        self.text(label).color(theme.foreground);
        self.pop_parent();
        let content_width = (width - label_width - theme.spacing.base * 3.0).max(1.0);
        let column = self.new_named_node(
            &format!("{id}:content"),
            Style {
                width: Some(content_width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.base * 1.5,
                ..Style::default()
            },
        );
        self.push_parent(column);
        content(self);
        // MyGo draws a field's description and message one point below the
        // theme's body size (`ui/form.go` fieldText: `FontSize - 1`).
        let field_text_size = theme.font_sizes.body - 1.0;
        if let Some(description) = description {
            self.text(description)
                .font_size(field_text_size)
                .color(theme.muted)
                .max_width(content_width);
        }
        if let Some(error) = error {
            self.text(error)
                .font_size(field_text_size)
                .color(theme.danger())
                .max_width(content_width);
        }
        self.pop_parent();
        self.pop_parent();
        self.register_access_node(
            root,
            Role::Group,
            label,
            Some(if has_error { "error" } else { "valid" }),
            false,
        );
        FormFieldState {
            has_error,
            has_description: description.is_some(),
        }
    }

    pub fn form_field(
        &mut self,
        id: &str,
        label: &str,
        description: Option<&str>,
        error: Option<&str>,
        content: impl FnOnce(&mut Context),
    ) -> FormFieldState {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let has_error = error.is_some();
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.xs,
                ..Style::default()
            },
        );
        self.push_parent(root);
        self.text(label)
            .weight(600)
            .color(if has_error {
                theme.danger()
            } else {
                theme.foreground
            })
            .max_width(width);
        if let Some(description) = description {
            self.text(description)
                .font_size(theme.font_sizes.small)
                .color(theme.muted)
                .max_width(width);
        }
        content(self);
        if let Some(error) = error {
            self.text(error)
                .font_size(theme.font_sizes.small)
                .color(theme.danger())
                .max_width(width);
        }
        self.pop_parent();
        self.register_access_node(
            root,
            Role::Group,
            label,
            Some(if has_error { "error" } else { "valid" }),
            false,
        );
        FormFieldState {
            has_error,
            has_description: description.is_some(),
        }
    }
}

fn compose_transform(parent: Transform, child: Transform) -> Transform {
    let scale = Point::new(
        parent.scale.x * child.scale.x,
        parent.scale.y * child.scale.y,
    );
    Transform {
        translation: transform_point(parent, child.translation),
        scale,
        rotation: parent.rotation + child.rotation,
    }
}

fn transform_point(transform: Transform, point: Point) -> Point {
    let cos = transform.rotation.cos();
    let sin = transform.rotation.sin();
    Point::new(
        cos * transform.scale.x * point.x - sin * transform.scale.y * point.y
            + transform.translation.x,
        sin * transform.scale.x * point.x
            + cos * transform.scale.y * point.y
            + transform.translation.y,
    )
}

fn inverse_transform_point(transform: Transform, point: Point, origin: Rect) -> Point {
    if transform.scale.x.abs() <= f32::EPSILON || transform.scale.y.abs() <= f32::EPSILON {
        return point;
    }
    let dx = point.x - origin.x - transform.translation.x;
    let dy = point.y - origin.y - transform.translation.y;
    let cos = (-transform.rotation).cos();
    let sin = (-transform.rotation).sin();
    let x = (cos * dx - sin * dy) / transform.scale.x;
    let y = (sin * dx + cos * dy) / transform.scale.y;
    Point::new(origin.x + x, origin.y + y)
}

fn rect_contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.x && point.y >= rect.y && point.x <= rect.right() && point.y <= rect.bottom()
}

struct PendingNode {
    style: Style,
    content_size: Size,
    children: Vec<u64>,
    commands: Vec<DrawCommand>,
    overlay_commands: Vec<DrawCommand>,
    scroll: Option<ScrollNode>,
    key: Option<String>,
    layer: u32,
    transform: Transform,
    rtl: bool,
    /// Face painted in the node's final box, before its own commands.
    fill: Option<(Color, Radius)>,
    /// One-pixel border painted around the node's final box.
    stroke: Option<(Color, Radius)>,
    /// Clips this node's children to its final box, as a scroll view's
    /// viewport does without scrolling.
    clip: bool,
    /// A one-pixel rule along the node's top edge, as MyGo's
    /// `BorderWidth(1, 0, 0, 0)` draws a section divider.
    top_border: Option<Color>,
    /// A one-pixel rule along the node's bottom edge, as MyGo's
    /// `BorderWidth(0, 0, 1, 0)` draws a find bar's rule.
    bottom_border: Option<Color>,
    /// Distance from the node's top to the baseline of its first line of
    /// text, for rows that line their children up by it (`ui/form.go`
    /// alignBaselines).
    first_baseline: Option<f32>,
    /// The ascent and descent of that line, which a row lines a control
    /// without text up on (`ui/form.go` anchor).
    text_metrics: Option<(f32, f32)>,
}

struct ScrollNode {
    offset: f32,
    viewport_height: f32,
}

/// Builder for a vertical flex container.
pub type ColumnBuilder<'a> = FlexBuilder<'a>;
/// Builder for a horizontal flex container.
pub type RowBuilder<'a> = FlexBuilder<'a>;
/// Builder for a neutral container.
pub type ContainerBuilder<'a> = FlexBuilder<'a>;
/// Builder for a grid container.
pub type GridBuilder<'a> = FlexBuilder<'a>;

/// Builder for a container that also takes pointer input, as a demo's own
/// list rows and menu items do.
pub struct ClickableBuilder<'a> {
    builder: FlexBuilder<'a>,
    /// Whether the pointer was over the container before this frame.
    pub hovered: bool,
    /// Whether the container was clicked before this frame.
    pub clicked: bool,
}

impl<'a> ClickableBuilder<'a> {
    /// Sets the gap between children.
    pub fn gap(mut self, gap: f32) -> Self {
        self.builder = self.builder.gap(gap);
        self
    }

    /// Sets padding on all sides.
    pub fn padding(mut self, padding: f32) -> Self {
        self.builder = self.builder.padding(padding);
        self
    }

    /// Sets horizontal padding only.
    pub fn padding_x(mut self, padding: f32) -> Self {
        self.builder = self.builder.padding_x(padding);
        self
    }

    /// Sets vertical padding only.
    pub fn padding_y(mut self, padding: f32) -> Self {
        self.builder = self.builder.padding_y(padding);
        self
    }

    /// Sets horizontal and vertical padding.
    pub fn padding_xy(mut self, x: f32, y: f32) -> Self {
        self.builder = self.builder.padding_xy(x, y);
        self
    }

    /// Sets padding per edge, in top, right, bottom and left order.
    pub fn padding_each(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.builder = self.builder.padding_each(top, right, bottom, left);
        self
    }

    /// Sets the background and its corner radius.
    pub fn background(mut self, color: Color, radius: f32) -> Self {
        self.builder = self.builder.background(color, radius);
        self
    }

    /// Sets a one-pixel border.
    pub fn border(mut self, color: Color, radius: f32) -> Self {
        self.builder = self.builder.border(color, radius);
        self
    }

    /// Sets an explicit size.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.builder = self.builder.size(width, height);
        self
    }

    /// Sets the width.
    pub fn width(mut self, width: f32) -> Self {
        self.builder = self.builder.width(width);
        self
    }

    /// Sets the height.
    pub fn height(mut self, height: f32) -> Self {
        self.builder = self.builder.height(height);
        self
    }

    /// Aligns children across the row.
    pub fn align(mut self, align: AlignItems) -> Self {
        self.builder = self.builder.align(align);
        self
    }

    /// Lays children out in a column instead of a row.
    pub fn column(mut self) -> Self {
        self.builder = self.builder.column();
        self
    }

    /// Distributes children along the row.
    pub fn justify(mut self, justify: JustifyContent) -> Self {
        self.builder = self.builder.justify(justify);
        self
    }

    /// Paints commands in the container's box.
    pub fn draw(mut self, commands: impl IntoIterator<Item = DrawCommand>) -> Self {
        self.builder = self.builder.draw(commands);
        self
    }

    /// Builds children with this container as their parent scope.
    pub fn children(mut self, build: impl FnOnce(&mut Context)) -> Self {
        self.builder = self.builder.children(build);
        self
    }
}

/// Immediate-mode flex container builder.
pub struct FlexBuilder<'a> {
    ctx: &'a mut Context,
    id: u64,
}

impl<'a> FlexBuilder<'a> {
    fn new(ctx: &'a mut Context, style: Style) -> Self {
        let id = ctx.new_node(style);
        Self { ctx, id }
    }

    fn node(&mut self) -> &mut PendingNode {
        self.ctx
            .nodes
            .get_mut(&self.id)
            .expect("builder node exists")
    }

    /// Sets the gap between children.
    pub fn gap(mut self, gap: f32) -> Self {
        self.node().style.gap = gap;
        self
    }

    /// Takes the container out of the flow, placing it at the given offset
    /// from its parent's content origin, as MyGo's `Absolute` and `Left`/
    /// `Top` do for a chart mark.
    pub fn absolute(mut self, left: f32, top: f32) -> Self {
        let node = self.node();
        node.style.position = Position::Absolute;
        node.style.left = Some(left);
        node.style.top = Some(top);
        self
    }

    /// Lays children out in a column instead of a row.
    pub fn column(mut self) -> Self {
        self.node().style.flex_direction = FlexDirection::Column;
        self
    }

    /// Sets the grid column count and enables grid placement.
    pub fn columns(mut self, columns: usize) -> Self {
        let node = self.node();
        node.style.display_grid = true;
        node.style.grid_columns = columns.max(1);
        self
    }

    /// Sets equal padding.
    pub fn padding(mut self, padding: f32) -> Self {
        self.node().style.padding = super::layout::Edges::all(padding);
        self
    }

    /// Sets horizontal padding only.
    pub fn padding_x(mut self, padding: f32) -> Self {
        let node = self.node();
        node.style.padding.left = padding;
        node.style.padding.right = padding;
        self
    }

    /// Sets vertical padding only.
    pub fn padding_y(mut self, padding: f32) -> Self {
        let node = self.node();
        node.style.padding.top = padding;
        node.style.padding.bottom = padding;
        self
    }

    /// Sets horizontal and vertical padding.
    pub fn padding_xy(mut self, x: f32, y: f32) -> Self {
        let node = self.node();
        node.style.padding.left = x;
        node.style.padding.right = x;
        node.style.padding.top = y;
        node.style.padding.bottom = y;
        self
    }

    /// Sets padding per edge, in top, right, bottom and left order.
    pub fn padding_each(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        let node = self.node();
        node.style.padding.top = top;
        node.style.padding.right = right;
        node.style.padding.bottom = bottom;
        node.style.padding.left = left;
        self
    }

    /// Sets equal margin.
    pub fn margin(mut self, margin: f32) -> Self {
        self.node().style.margin = super::layout::Edges::all(margin);
        self
    }

    /// Sets margin per edge, in top, right, bottom and left order.
    pub fn margin_each(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.node().style.margin = super::layout::Edges::new(top, right, bottom, left);
        self
    }

    /// Sets an explicit width and height.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        let node = self.node();
        node.style.width = Some(width);
        node.style.height = Some(height);
        self
    }

    /// Sets an explicit width.
    pub fn width(mut self, width: f32) -> Self {
        self.node().style.width = Some(width);
        self
    }

    /// Sets an explicit height.
    pub fn height(mut self, height: f32) -> Self {
        self.node().style.height = Some(height);
        self
    }

    /// Fills the current available size.
    pub fn fill(mut self) -> Self {
        let size = self.ctx.available_size;
        let node = self.node();
        node.style.width = Some(size.width);
        node.style.height = Some(size.height);
        self
    }

    /// Fills the parent's content box, as MyGo's `Fill` does.
    ///
    /// Unlike [`Self::fill`], which takes the window's size, this grows into
    /// whatever room the parent lays out: the flex growth takes the main
    /// axis and the default stretch alignment the cross axis.
    pub fn fill_parent(mut self) -> Self {
        let node = self.node();
        node.style.flex_grow = 1.0;
        node.style.width = None;
        node.style.height = None;
        self
    }

    /// Centers children on both axes.
    pub fn center(mut self) -> Self {
        let node = self.node();
        node.style.align_items = AlignItems::Center;
        node.style.justify_content = JustifyContent::Center;
        self
    }

    /// Sets cross-axis alignment.
    pub fn align(mut self, align: AlignItems) -> Self {
        self.node().style.align_items = align;
        self
    }

    /// Asks the parent to align this container across its axis instead of
    /// the parent's own alignment, as MyGo's `AlignSelf`.
    pub fn align_self(mut self, align: AlignItems) -> Self {
        self.node().style.align_self = Some(align);
        self
    }

    /// Sets main-axis distribution.
    pub fn justify(mut self, justify: JustifyContent) -> Self {
        self.node().style.justify_content = justify;
        self
    }

    /// Wraps this row's children onto more lines when they overflow,
    /// centering each line as MyGo's `Wrap` does.
    pub fn wrap(mut self) -> Self {
        self.node().style.flex_wrap = true;
        self
    }

    /// Registers this container as a hover/context-menu target under `id`.
    pub fn interactive(self, stable: &str) -> Self {
        self.ctx.register_interaction(self.id, stable.to_string());
        self
    }

    /// Paints a face in this container's final layout box, before its
    /// own drawing commands and children.
    pub fn background(mut self, color: Color, radius: f32) -> Self {
        self.node().fill = Some((color, Radius::all(radius)));
        self
    }

    /// Paints a one-pixel border around this container's final box.
    /// Requires [`Self::background`] for the inner face.
    pub fn border(mut self, color: Color, radius: f32) -> Self {
        self.node().stroke = Some((color, Radius::all(radius)));
        self
    }

    /// Sets flexible growth.
    pub fn flex_grow(mut self, grow: f32) -> Self {
        self.node().style.flex_grow = grow;
        self
    }

    /// Adds this container to keyboard focus traversal order.
    pub fn focusable(self) -> Self {
        let key = self.ctx.node_key(self.id);
        self.ctx.register_interaction(self.id, key.clone());
        self.ctx.register_focusable(self.id, key);
        self
    }

    /// Adds local drawing commands to this container.
    pub fn draw(mut self, commands: impl IntoIterator<Item = DrawCommand>) -> Self {
        self.node().commands.extend(commands);
        self
    }

    /// Clips this container's children to its final box.
    ///
    /// MyGo's List is a Scroll: its rows scroll under a fixed viewport and
    /// anything past the edge is cut. A demo that lays out MyGo's rows by
    /// hand needs the same cut, without the wheel handling.
    pub fn clip(self) -> Self {
        self.ctx.nodes.get_mut(&self.id).expect("clip node exists").clip = true;
        self
    }

    /// Builds children with this container as their parent scope.
    pub fn children(self, build: impl FnOnce(&mut Context)) -> Self {
        self.ctx.push_parent(self.id);
        build(self.ctx);
        self.ctx.pop_parent();
        self
    }

    /// Builds one child with this container as its parent scope.
    pub fn child(self, build: impl FnOnce(&mut Context)) -> Self {
        self.children(build)
    }
}

/// Text widget builder.
pub struct TextBuilder<'a> {
    ctx: &'a mut Context,
    id: u64,
    text: String,
    font_size: f32,
    weight: u16,
    color: Color,
    max_width: Option<f32>,
    alignment: TextAlignment,
    underline: bool,
    /// Height of the box the line sits in, its glyphs centered in it, as
    /// MyGo's line() centers a line taller than its faces.
    line_height: Option<f32>,
    /// Whether the text stays on one line, as MyGo's SingleLine does,
    /// instead of wrapping at its maximum width.
    single_line: bool,
    /// Whether the text uses the monospace family, as MyGo's `Font("monospace")`.
    mono: bool,
    /// Named font family, as MyGo's `Font(name)` takes it.
    family: Option<String>,
}

impl TextBuilder<'_> {
    /// Sets font size in logical pixels.
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self.sync();
        self
    }

    /// Uses bold weight.
    pub fn bold(mut self) -> Self {
        self.weight = 700;
        self.sync();
        self
    }

    /// Sets numeric font weight.
    pub fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self.sync();
        self
    }

    /// Sets text color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self.sync();
        self
    }

    /// Sets maximum width for wrapping.
    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = Some(width);
        self.sync();
        self
    }

    /// Sets horizontal text alignment.
    pub fn align(mut self, alignment: TextAlignment) -> Self {
        self.alignment = alignment;
        self.sync();
        self
    }

    /// Underlines the text with a one-pixel line beneath its ascent.
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self.sync();
        self
    }

    /// Sets the height of the line the text sits in, centering its glyphs
    /// as MyGo does in a row taller than its text.
    pub fn line_height(mut self, height: f32) -> Self {
        self.line_height = (height > 0.0).then_some(height);
        self.sync();
        self
    }

    /// Sets the monospace family, as MyGo's `Font("monospace")`.
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self.sync();
        self
    }

    /// Names the font family, as MyGo's `Font(name)` takes it. The name is
    /// resolved by the host canvas, so a system family such as "Georgia"
    /// draws in that face.
    pub fn font(mut self, family: impl Into<String>) -> Self {
        let family = family.into();
        self.mono = family.eq_ignore_ascii_case("monospace");
        self.family = Some(family);
        self.sync();
        self
    }

    /// Keeps the text on one line, as MyGo's SingleLine does: its maximum
    /// width bounds the line's box rather than wrapping it.
    pub fn single_line(mut self) -> Self {
        self.single_line = true;
        self.sync();
        self
    }

    /// Sets padding per edge, in top, right, bottom and left order.
    pub fn padding_each(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        if let Some(node) = self.ctx.nodes.get_mut(&self.id) {
            node.style.padding = super::layout::Edges::new(top, right, bottom, left);
        }
        self.sync();
        self
    }

    /// Sets an explicit width, as MyGo's Width does for text that centers
    /// in a wider column than its glyphs need.
    pub fn width(mut self, width: f32) -> Self {
        if let Some(node) = self.ctx.nodes.get_mut(&self.id) {
            node.style.width = Some(width);
        }
        self.sync();
        self
    }

    /// Adds this text widget to keyboard focus traversal order.
    pub fn focusable(self) -> Self {
        let key = self.ctx.node_key(self.id);
        self.ctx.register_interaction(self.id, key.clone());
        self.ctx.register_focusable(self.id, key);
        self
    }

    fn sync(&mut self) {
        // The line box sits inside the padding, so a fixed line height is
        // measured without it, as MyGo's text is (`ui/element.go` Padding).
        let padding_vertical = self
            .ctx
            .nodes
            .get(&self.id)
            .map(|node| node.style.padding.top + node.style.padding.bottom)
            .unwrap_or(0.0);
        let offset_y = self
            .line_height
            .map(|height| {
                ((height - padding_vertical - measured_text_height(self.font_size)) / 2.0)
                    .max(0.0)
            })
            .unwrap_or(0.0);
        if let Some(height) = self.line_height {
            if let Some(node) = self.ctx.nodes.get_mut(&self.id) {
                node.style.height = Some(height);
            }
        }
        self.ctx.update_text(
            self.id,
            &self.text,
            self.font_size,
            self.weight,
            self.color,
            self.max_width,
            self.alignment,
            offset_y,
            self.single_line,
            self.mono,
            self.family.as_deref(),
        );
        if self.underline {
            let width = measure_text_width(&self.text, self.font_size);
            let ascent = default_font()
                .map(|font| font.ascent(self.font_size))
                .unwrap_or(self.font_size * 0.93);
            if let Some(node) = self.ctx.nodes.get_mut(&self.id) {
                node.commands.push(DrawCommand::Line {
                    x1: 0.0,
                    y1: ascent + 2.0,
                    x2: width,
                    y2: ascent + 2.0,
                    width: 1.0,
                    color: self.color,
                });
            }
        }
    }
}

/// Result of constructing a button for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ButtonState {
    /// Stable interaction id.
    pub id: String,
    /// Whether the primary button was released inside this button this frame.
    pub clicked: bool,
    /// Whether the pointer is currently held inside the button.
    pub pressed: bool,
    /// Whether the button currently owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the button.
    pub hovered: bool,
    /// Previous-frame bounds, when available.
    pub bounds: Option<Rect>,
}

impl ButtonState {
    /// Keeps this button in focus traversal order.
    ///
    /// Buttons are focusable by default; this method exists for builder-style
    /// symmetry with other widgets.
    pub fn focusable(self) -> Self {
        self
    }
}

/// State returned by a text field for the current frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFieldState {
    /// Whether editing changed the value.
    pub changed: bool,
    /// Current byte offset of the text cursor.
    pub cursor: usize,
    /// Whether the field owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the field.
    pub hovered: bool,
}

/// State returned by a checkbox for the current frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckboxState {
    /// Whether the checkbox value changed this frame.
    pub changed: bool,
    /// Value after applying this frame's input.
    pub checked: bool,
    /// Whether the checkbox owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the checkbox.
    pub hovered: bool,
}

/// State returned by a slider for the current frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentState {
    /// Whether the segment was clicked or activated by Space.
    pub clicked: bool,
    /// Whether the pointer is over the segment.
    pub hovered: bool,
    /// Whether the pointer is held down on the segment.
    pub pressed: bool,
    /// Whether the segment owns keyboard focus.
    pub focused: bool,
}

pub struct RadioState {
    /// Whether the radio was clicked, choosing its value.
    pub clicked: bool,
    /// Whether the pointer is over the radio.
    pub hovered: bool,
    /// Whether the radio owns keyboard focus.
    pub focused: bool,
}

pub enum SegmentFace {
    /// A standalone toggle: Surface face with MyGo's 1px border, pressed
    /// staying SurfacePressed.
    Plain,
    /// A segment in a toggle group or segmented track: transparent until
    /// hovered, painting the theme background when chosen.
    Segment,
    /// A toolbar button: no face or border of its own until hovered, sunk
    /// to SurfacePressed while on, as MyGo's toolbarButtons style.
    Toolbar,
}

pub struct SliderState {
    /// Whether the value changed this frame.
    pub changed: bool,
    /// Whether the pointer is dragging the slider.
    pub dragging: bool,
    /// Whether the slider owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the slider.
    pub hovered: bool,
}

/// The face of a scroll view, as MyGo's Scroll takes it: a background, its
/// border and radius, and the padding and gap inside.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollStyle {
    /// Fill color behind the content.
    pub background: Option<Color>,
    /// One-pixel border around the fill.
    pub border: Option<Color>,
    /// Corner radius of the fill and border.
    pub radius: f32,
    /// Space inside the edges.
    pub padding: super::layout::Edges,
    /// Space between children.
    pub gap: f32,
    /// Offset to scroll to on the first frame, as MyGo's
    /// `TrackScroll` state's `Y` starts at its maximum: the log's end.
    pub initial_offset: Option<f32>,
}

impl Default for ScrollStyle {
    fn default() -> Self {
        Self {
            background: None,
            border: None,
            radius: 0.0,
            padding: super::layout::Edges::default(),
            gap: 0.0,
            initial_offset: None,
        }
    }
}

/// State returned by a scroll view for the current frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollState {
    /// Current vertical scroll offset.
    pub scroll: f32,
    /// Maximum valid scroll offset.
    pub max_scroll: f32,
    /// Whether scrolling changed this frame.
    pub changed: bool,
    /// Whether the scrollbar thumb is being dragged.
    pub dragging: bool,
    /// Whether the pointer is over the scroll viewport.
    pub hovered: bool,
}

/// Applies a transform scope to widgets constructed inside it.
pub fn transform(context: &mut Context, transform: Transform, content: impl FnOnce(&mut Context)) {
    context.transform(transform, content);
}

/// Creates a vertical flex container.
pub fn column(context: &mut Context) -> ColumnBuilder<'_> {
    context.column()
}

/// Starts a grid container.
pub fn grid(context: &mut Context) -> GridBuilder<'_> {
    context.grid()
}

/// Creates a horizontal flex container.
pub fn row(context: &mut Context) -> RowBuilder<'_> {
    context.row()
}

/// Creates a neutral container.
/// Expands the tabs of a line to the next multiple of four columns, as
/// MyGo's editor does with its default `tabSize`.
fn expand_tabs(line: &str) -> String {
    if !line.contains('\t') {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len() + 8);
    let mut column = 0;
    for c in line.chars() {
        if c == '\t' {
            let next = (column / 4 + 1) * 4;
            for _ in column..next {
                out.push(' ');
            }
            column = next;
        } else {
            out.push(c);
            column += 1;
        }
    }
    out
}

/// Expands the tabs of the prefix of `line` before byte `at`.
fn expand_tabs_prefix(line: &str, at: usize) -> String {
    let at = at.min(line.len());
    let mut cut = at;
    while cut > 0 && !line.is_char_boundary(cut) {
        cut -= 1;
    }
    expand_tabs(&line[..cut])
}

/// The tints and marks of MyGo's diff viewer: the changed lines' colors
/// mixed in Oklab over white, and the bars and marks of the changes.
const ADDED_LINE: Color = Color::rgb(233, 248, 234);
const ADDED_GUTTER: Color = Color::rgb(238, 250, 239);
const DELETED_LINE: Color = Color::rgb(255, 234, 232);
const DELETED_GUTTER: Color = Color::rgb(255, 239, 238);
const ADDED_BAR: Color = Color::rgb(13, 190, 78);
const DELETED_BAR: Color = Color::rgb(255, 46, 63);
const MODIFIED: Color = Color::rgb(9, 159, 255);
const LINE_NUMBER: Color = Color::rgb(149, 149, 154);
const DIVIDER: Color = Color::rgb(244, 244, 245);
/// The look of one diff line: its tint, its gutter's, its change bar's,
/// and its number's, as MyGo's `lineLook`.
#[derive(Debug, Clone, Copy, Default)]
struct LineLook {
    bg: Option<Color>,
    gutter: Option<Color>,
    bar: Option<Color>,
    striped: bool,
    number: Option<Color>,
}

impl LineLook {
    fn same() -> Self {
        LineLook::default()
    }

    fn added() -> Self {
        LineLook {
            bg: Some(ADDED_LINE),
            gutter: Some(ADDED_GUTTER),
            bar: Some(ADDED_BAR),
            striped: false,
            number: Some(ADDED_BAR),
        }
    }

    fn removed() -> Self {
        LineLook {
            bg: Some(DELETED_LINE),
            gutter: Some(DELETED_GUTTER),
            bar: Some(DELETED_BAR),
            striped: true,
            number: Some(DELETED_BAR),
        }
    }
}

/// The geometry every line of a diff is drawn at.
#[derive(Debug, Clone, Copy)]
struct DiffGeometry {
    gutter: f32,
    text_x: f32,
    line_height: f32,
    line_top: f32,
    code_size: f32,
    scrollbar: f32,
}

/// Draws the hatched filler of the rows a side of a split view lacks:
/// stripes at -45 degrees, every 8 DIPs, as Pierre's diffs.
fn push_filler(
    commands: &mut Vec<DrawCommand>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    gutter: f32,
    color: Color,
) {
    let left = x + gutter;
    let right = x + width;
    if right <= left || height <= 0.0 {
        return;
    }
    // The stripes are those of the whole block, anchored at its
    // bottom-left corner, so that they run on across its rows as MyGo's
    // do (`fillerStripes`).
    let bottom = y + height;
    let phase = left + bottom;
    let mut s = phase + 8.0 * ((left + y - phase) / 8.0).floor() + 8.0;
    while s < right + bottom {
        // One line across the block, as MyGo strokes it
        // (`ui/editor/diffside.go` fillerStripes): the visible rect's clip
        // cuts it at the sides, and cutting it at each row instead would
        // leave the seam of every butt cap showing.
        // MyGo clips the filler to the text column, left of the gutter;
        // the stripes cross that edge, so each piece starts where it
        // enters and ends where it leaves.
        let y0 = bottom.min(s - left);
        let y1 = y.max(s - right);
        let x0 = s - y0;
        let x1 = s - y1;
        if x1 > x0 {
            commands.push(DrawCommand::Line {
                x1: x0,
                y1: y0,
                x2: x1,
                y2: y1,
                width: 1.0,
                color,
            });
        }
        s += 8.0;
    }
}

pub fn container(context: &mut Context) -> ContainerBuilder<'_> {
    context.container()
}

/// Creates a text widget.
pub fn text<'a>(context: &'a mut Context, content: &str) -> TextBuilder<'a> {
    context.text(content)
}

/// Creates a button.
pub fn button(context: &mut Context, label: &str) -> ButtonState {
    context.button(label)
}

/// Creates a button with explicit padding, as MyGo's `Padding` on a plain
/// button makes a numeric input's − and + buttons.
pub fn button_padded(
    context: &mut Context,
    id: &str,
    label: &str,
    horizontal: f32,
    vertical: f32,
    active: bool,
) -> ButtonState {
    context.button_padded(id, label, horizontal, vertical, active)
}

/// Creates a padded button that shows as unavailable when `disabled` is set.
pub fn button_padded_disabled(
    context: &mut Context,
    id: &str,
    label: &str,
    horizontal: f32,
    vertical: f32,
    active: bool,
    disabled: bool,
) -> ButtonState {
    context.button_padded_disabled(id, label, horizontal, vertical, active, disabled)
}

/// Creates a button that shows as unavailable when `disabled` is set.
pub fn button_disabled(context: &mut Context, label: &str, disabled: bool) -> ButtonState {
    context.button_disabled(label, disabled)
}

/// Creates a filled primary button.
pub fn button_primary(context: &mut Context, label: &str) -> ButtonState {
    context.button_primary(label)
}

/// Creates a button with a built-in icon before the label.
pub fn button_with_icon(context: &mut Context, label: &str, icon_name: &str) -> ButtonState {
    context.button_with_icon(label, icon_name)
}


/// Creates a 36×36 icon-only button.
pub fn icon_button(context: &mut Context, icon_name: &str) -> ButtonState {
    context.icon_button(icon_name)
}

/// Creates a primary button with a built-in icon.
pub fn button_primary_with_icon(
    context: &mut Context,
    label: &str,
    icon_name: &str,
) -> ButtonState {
    context.button_primary_with_icon(label, icon_name)
}

/// Creates a flexible spacer.
pub fn spacer(context: &mut Context) {
    context.spacer();
}

/// Creates a text field.
pub fn text_field_invalid(
    context: &mut Context,
    id: &str,
    value: &mut String,
    invalid: bool,
) -> TextFieldState {
    context.text_field_invalid(id, value, invalid)
}

/// Creates a single-line text field.
pub fn text_field(context: &mut Context, id: &str, value: &mut String) -> TextFieldState {
    context.text_field(id, value)
}

/// Creates a single-line password field that masks the rendered characters.
pub fn text_field_password(
    context: &mut Context,
    id: &str,
    value: &mut String,
) -> TextFieldState {
    context.text_field_password(id, value)
}

/// Creates a multiline text area.
pub fn text_area(
    context: &mut Context,
    id: &str,
    value: &mut String,
    visible_lines: usize,
) -> TextFieldState {
    context.text_area(id, value, visible_lines)
}

/// Creates a checkbox.
pub fn checkbox(context: &mut Context, id: &str, checked: &mut bool) -> CheckboxState {
    context.checkbox(id, checked)
}

/// Creates a checkbox with a visible label.
pub fn checkbox_labeled(
    context: &mut Context,
    id: &str,
    label: &str,
    checked: &mut bool,
) -> CheckboxState {
    context.checkbox_labeled(id, label, checked)
}

/// Creates a labeled checkbox in the mixed/indeterminate visual state.
pub fn checkbox_indeterminate(
    context: &mut Context,
    id: &str,
    label: &str,
    checked: &mut bool,
    indeterminate: bool,
) -> CheckboxState {
    context.checkbox_indeterminate(id, label, checked, indeterminate)
}

pub fn slider(context: &mut Context, id: &str, value: &mut f32, min: f32, max: f32) -> SliderState {
    context.slider(id, value, min, max)
}

/// Creates a button with a trailing, muted icon.
pub fn button_with_icon_after(
    context: &mut Context,
    label: &str,
    icon_name: &str,
) -> ButtonState {
    context.button_with_icon_after(label, icon_name)
}

/// Creates a text field that shows `placeholder` while empty.
pub fn text_field_placeholder(
    context: &mut Context,
    id: &str,
    value: &mut String,
    placeholder: &str,
) -> TextFieldState {
    context.text_field_placeholder(id, value, placeholder)
}

/// Creates a masked field that shows `placeholder` while empty.
pub fn text_field_password_placeholder(
    context: &mut Context,
    id: &str,
    value: &mut String,
    placeholder: &str,
) -> TextFieldState {
    context.text_field_password_placeholder(id, value, placeholder)
}

/// Creates a multiline text area that shows `placeholder` while empty.
pub fn text_area_placeholder(
    context: &mut Context,
    id: &str,
    value: &mut String,
    visible_lines: usize,
    placeholder: &str,
) -> TextFieldState {
    context.text_area_placeholder(id, value, visible_lines, placeholder)
}

/// Creates a multiline text area at least `min_height` tall.
pub fn text_area_sized(
    context: &mut Context,
    id: &str,
    value: &mut String,
    visible_lines: usize,
    min_height: f32,
) -> TextFieldState {
    context.text_area_sized(id, value, visible_lines, min_height)
}

/// Creates a multiline text area at least `min_height` tall that shows a
/// placeholder while empty, as MyGo's `TextArea(...).Placeholder`.
pub fn text_area_sized_placeholder(
    context: &mut Context,
    id: &str,
    value: &mut String,
    visible_lines: usize,
    min_height: f32,
    placeholder: &str,
) -> TextFieldState {
    context.text_area_sized_placeholder(id, value, visible_lines, min_height, placeholder)
}

/// Creates a monospace multiline text area, as MyGo's
/// `TextAreaBuffer(...).Font("monospace")`.
pub fn text_area_mono(
    context: &mut Context,
    id: &str,
    value: &mut String,
    visible_lines: usize,
    min_height: f32,
) -> TextFieldState {
    context.text_area_mono(id, value, visible_lines, min_height)
}

/// Creates a stepped slider with tick marks, as MyGo's StepSlider.
pub fn slider_stepped(
    context: &mut Context,
    id: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    step: f32,
) -> SliderState {
    context.slider_stepped(id, value, min, max, step)
}

/// Creates a radio button reporting clicks without owning the choice.
pub fn radio(
    context: &mut Context,
    id: &str,
    label: &str,
    selected: &mut usize,
    value: usize,
) -> RadioState {
    context.radio(id, label, selected, value)
}

/// Creates a MyGo segment reporting clicks without owning the choice.
#[allow(clippy::too_many_arguments)]
pub fn segment(
    context: &mut Context,
    id: &str,
    label: &str,
    icon: Option<&str>,
    selected: bool,
    padding: Edges,
    radius: f32,
    face: SegmentFace,
    disabled: bool,
) -> SegmentState {
    context.segment(id, label, icon, selected, padding, radius, face, disabled)
}

/// Creates a vertically scrollable region.
pub fn scroll_view(
    context: &mut Context,
    id: &str,
    viewport_height: f32,
    content: impl FnOnce(&mut Context),
) -> ScrollState {
    context.scroll_view(id, viewport_height, content)
}

/// Creates a scroll view with a face, as MyGo's Scroll takes Background,
/// Border, Radius and Padding.
pub fn scroll_view_styled(
    context: &mut Context,
    id: &str,
    viewport_height: f32,
    style: ScrollStyle,
    content: impl FnOnce(&mut Context),
) -> ScrollState {
    context.scroll_view_styled(id, viewport_height, style, content)
}

fn snap_slider_value(value: f32, min: f32, max: f32, step: f32) -> f32 {
    if step <= 0.0 {
        return value;
    }
    let mut snapped = min + ((value - min) / step).round() * step;
    if snapped > max {
        snapped -= step;
    }
    snapped.clamp(min, max)
}

/// Counts the tick marks of a slider, 0 for no step or too many to tell
/// apart, as MyGo's `tickCount`.
fn slider_tick_count(min: f32, max: f32, step: f32) -> usize {
    if step <= 0.0 || max <= min {
        return 0;
    }
    let n = ((max - min) / step + 1e-9).floor() as usize + 1;
    if n > 61 {
        0
    } else {
        n
    }
}

/// Returns a one-pixel border of a rounded box as one filled path: the
/// box's outline with the outline a pixel inside it wound the other way, so
/// the nonzero fill leaves only the band between them. MyGo draws a border
/// the same way (`internal/scene` OpFill's Border), and unlike a stroked
/// path it covers whole pixel cells with no half-pixel seam.
fn rounded_ring(width: f32, height: f32, radius: f32, border: f32, color: Color) -> DrawCommand {
    let inner_width = (width - border * 2.0).max(0.0);
    let inner_height = (height - border * 2.0).max(0.0);
    let inner_radius = (radius - border).clamp(0.0, inner_width.min(inner_height) / 2.0);
    let outer_radius = radius.clamp(0.0, width.min(height) / 2.0);
    let path = format!(
        // The outer edge, clockwise.
        "M{r} 0H{w1}A{r} {r} 0 0 1 {w} {r}V{h1}A{r} {r} 0 0 1 {w1} {h}H{r}\
         A{r} {r} 0 0 1 0 {h1}V{r}A{r} {r} 0 0 1 {r} 0Z\
         M{ix} {iy2}V{ih1}A{ir} {ir} 0 0 0 {ix2} {ih}H{iw1}\
         A{ir} {ir} 0 0 0 {iw} {ih1}V{iy2}A{ir} {ir} 0 0 0 {iw1} {iy}\
         H{ix2}A{ir} {ir} 0 0 0 {ix} {iy2}Z",
        r = outer_radius,
        w1 = width - outer_radius,
        w = width,
        h1 = height - outer_radius,
        h = height,
        // The inner edge, counterclockwise from its top-left arc's end.
        ix = border,
        iy = border,
        iw = border + inner_width,
        ih = border + inner_height,
        ir = inner_radius,
        ix2 = border + inner_radius,
        iy2 = border + inner_radius,
        iw1 = border + inner_width - inner_radius,
        ih1 = border + inner_height - inner_radius,
    );
    DrawCommand::SvgPath {
        x: 0.0,
        y: 0.0,
        width,
        height,
        path,
        fill: Some(color),
        stroke: None,
        stroke_width: 0.0,
    }
}

/// Returns how thick MyGo strikes text of a size: a fourteenth of it, at
/// least a pixel (`internal/text/canvas_js.go` font, `thick`).
fn strike_thickness(font_size: f32) -> f32 {
    (font_size / 14.0).max(1.0)
}


fn stroked_rounded_rect(width: f32, height: f32, radius: f32, color: Color) -> DrawCommand {
    // A one-pixel border fills the outermost pixel cells, as MyGo's filled
    // borders do: the stroke center sits half a pixel in, so the ring covers
    // [0, 1) rather than straddling the edge and blurring across two pixels.
    let inset = 0.5_f32.min(width.min(height) / 2.0);
    let inner_width = (width - inset * 2.0).max(0.0);
    let inner_height = (height - inset * 2.0).max(0.0);
    let r = (radius - inset).clamp(0.0, inner_width.min(inner_height) / 2.0);
    let path = format!(
        "M{r} 0H{w1}A{r} {r} 0 0 1 {w} {r}V{h1}A{r} {r} 0 0 1 {w1} {h}H{r}A{r} {r} 0 0 1 0 {h1}V{r}A{r} {r} 0 0 1 {r} 0Z",
        r = r,
        w1 = inner_width - r,
        w = inner_width,
        h1 = inner_height - r,
        h = inner_height,
    );
    DrawCommand::SvgPath {
        x: inset,
        y: inset,
        width: inner_width,
        height: inner_height,
        path,
        fill: None,
        stroke: Some(color),
        stroke_width: 1.0,
    }
}

fn button_commands(
    label: &str,
    font_size: f32,
    theme: &Theme,
    hover_progress: f32,
    pressed_progress: f32,
    focused: bool,
    primary: bool,
    disabled: bool,
) -> Vec<DrawCommand> {
    let size = estimated_text_size(label, font_size, None);
    let hover_progress = hover_progress.clamp(0.0, 1.0);
    let pressed_progress = pressed_progress.clamp(0.0, 1.0);
    let hover_background = if primary {
        theme.accent_hover
    } else {
        theme.surface_hover
    };
    let pressed_background = if primary {
        theme.accent_pressed
    } else {
        theme.surface_pressed
    };
    let background = mix_color(
        mix_color(
            if primary { theme.accent } else { theme.surface },
            hover_background,
            hover_progress,
        ),
        pressed_background,
        pressed_progress,
    );
    let text_color = if primary {
        theme.on_accent
    } else {
        theme.foreground
    };
    let horizontal = if primary { 14.0 } else { 15.0 };
    let vertical = if primary { 6.0 } else { 7.0 };
    let width = size.width + horizontal * 2.0;
    let height = size.height + vertical * 2.0;
    let mut commands = Vec::new();
    if !primary {
        commands.push(DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
            radius: Radius::all(6.0),
            color: theme.border,
        });
    }
    commands.push(DrawCommand::RoundedRect {
        x: if primary { 0.0 } else { 1.0 },
        y: if primary { 0.0 } else { 1.0 },
        width: if primary { width } else { (width - 2.0).max(1.0) },
        height: if primary { height } else { (height - 2.0).max(1.0) },
        radius: Radius::all(if primary { 6.0 } else { 5.0 }),
        color: background,
    });
    commands.push(DrawCommand::Text {
        x: horizontal,
        y: vertical,
        text: label.to_string(),
        font_size,
        italic: false,
        mono: false,
        family: None,
        // MyGo's buttons take the theme's text as it is: its `styleButton`
        // only sets the color, so the label stays at weight 400.
        weight: 400,
        color: text_color,
        max_width: None,
        alignment: TextAlignment::Start,
    });
    if focused {
        commands.extend(focus_ring(
            0.0,
            0.0,
            width,
            height,
            theme.accent,
        ));
    }
    if disabled {
        // MyGo draws a disabled element and its contents into a layer at
        // half opacity (`Opacity(0.5)`), so each color blends once with the
        // page behind it.
        let dim = |color: Color| mix_color(color, theme.background, 0.5);
        for command in &mut commands {
            match command {
                DrawCommand::RoundedRect { color, .. }
                | DrawCommand::Rect { color, .. }
                | DrawCommand::Line { color, .. } => *color = dim(*color),
                DrawCommand::Text { color, .. } => *color = dim(*color),
                _ => {}
            }
        }
    }
    commands
}

/// Returns a circle's outline as a ring of the given stroke width, drawn
/// with the painter's own geometry: a filled disc and a hole.
fn stroked_circle(x: f32, y: f32, radius: f32, width: f32, color: Color) -> DrawCommand {
    DrawCommand::SvgPath {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        path: format!(
            "M{} {}a{r} {r} 0 1 0 {} 0a{r} {r} 0 1 0 {} 0Z",
            x - radius,
            y,
            radius * 2.0,
            -radius * 2.0,
            r = radius
        ),
        fill: None,
        stroke: Some(color),
        stroke_width: width,
    }
}

/// Returns the gray squares that show transparency, as MyGo's `checkers`
/// paints them: white behind, and 204 gray on every other square.
///
/// MyGo paints the checkerboard inside the track's capsule clip, which cuts
/// the squares that reach the round ends; a corner square is then exactly
/// the quarter disc of the capsule's end, so it is drawn as that disc. The
/// cut only lands on a whole square when the square is as wide as the
/// capsule's radius, which is the size these tracks use; otherwise the
/// square is left uncut.
fn checkerboard_commands(
    width: f32,
    y: f32,
    height: f32,
    size: f32,
    radius: f32,
) -> Vec<DrawCommand> {
    let mut commands = vec![DrawCommand::Rect {
        x: 0.0,
        y,
        width,
        height,
        color: Color::rgb(255, 255, 255),
    }];
    let columns = (width / size).ceil() as usize;
    let rows = (height / size).ceil() as usize;
    for row in 0..rows {
        for column in 0..columns {
            if (row + column) % 2 == 0 {
                let x = column as f32 * size;
                let square_y = y + row as f32 * size;
                let square_width = size.min(width - x);
                let square_height = size.min(y + height - square_y);
                let at_left = x <= 0.0;
                let at_right = x + square_width >= width;
                let at_top = square_y <= y;
                let at_bottom = square_y + square_height >= y + height;
                let cuts = radius > 0.0
                    && (square_width - radius).abs() < 0.01
                    && (square_height - radius).abs() < 0.01
                    && (at_left || at_right)
                    && (at_top || at_bottom);
                if cuts {
                    let s = radius;
                    let path = match (at_left, at_top) {
                        (true, true) => format!("M{s},0A{s},{s} 0 0 0 0,{s}L{s},{s}Z"),
                        (false, true) => format!("M0,0A{s},{s} 0 0 1 {s},{s}L0,{s}Z"),
                        (true, false) => format!("M0,{s}A{s},{s} 0 0 0 {s},{s}L{s},0Z"),
                        (false, false) => format!("M{s},0A{s},{s} 0 0 1 0,{s}L0,0Z"),
                    };
                    commands.push(DrawCommand::SvgPath {
                        x,
                        y: square_y,
                        width: square_width,
                        height: square_height,
                        path,
                        fill: Some(Color::rgb(204, 204, 204)),
                        stroke: None,
                        stroke_width: 0.0,
                    });
                } else {
                    commands.push(DrawCommand::Rect {
                        x,
                        y: square_y,
                        width: square_width,
                        height: square_height,
                        color: Color::rgb(204, 204, 204),
                    });
                }
            }
        }
    }
    commands
}

/// Sets a color panel's color from a new hue, saturation, value and alpha,
/// keeping the panel's own state in step, and reports whether the color
/// changed. Clamping happens here so every control agrees on the bounds.
fn set_hsv(state: &mut ColorPanelState, color: &mut Color, next: Hsv) -> bool {
    let next = next.clamped();
    state.hsv = next;
    let value = next.to_color();
    let changed = value != *color;
    if changed {
        *color = value;
    }
    state.last = *color;
    state.hex = hex_of(*color);
    changed
}

fn hsv_to_color(hue: f32, saturation: f32, value: f32) -> Color {    let hue = if hue.is_finite() {
        hue.rem_euclid(360.0)
    } else {
        0.0
    };
    let saturation = if saturation.is_finite() {
        saturation.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let value = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let sector = hue / 60.0;
    let chroma = value * saturation;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (red, green, blue) = match sector as u32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let base = value - chroma;
    let channel = |part: f32| ((part + base) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::rgb(channel(red), channel(green), channel(blue))
}

fn rgb_to_hsv(color: Color) -> (u16, u8, u8) {
    let red = f32::from(color.r) / 255.0;
    let green = f32::from(color.g) / 255.0;
    let blue = f32::from(color.b) / 255.0;
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let delta = maximum - minimum;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if maximum == red {
        60.0 * ((green - blue) / delta).rem_euclid(6.0)
    } else if maximum == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    let saturation = if maximum <= f32::EPSILON {
        0.0
    } else {
        delta / maximum
    };
    (
        (hue.rem_euclid(360.0).round() as u16) % 360,
        (saturation * 100.0).round().clamp(0.0, 100.0) as u8,
        (maximum * 100.0).round().clamp(0.0, 100.0) as u8,
    )
}

/// Five-point star in a 24x24 view box, used by the rating control.
/// A five-pointed star in a 24-unit box, as MyGo's `starPath` draws it
/// (`ui/indicators.go`): its points reach 0.48 of the width from the center,
/// its inner corners 0.2, and the center sits 0.04 of the height low.
/// Returns MyGo's five-pointed star in a box `size` wide and tall
/// (`ui/indicators.go` starPath): points on a circle 0.48 of the width out,
/// turning at one 0.2 out, about a center a twenty-fifth of the height below
/// the middle.
fn star_path(size: f32) -> String {
    let center_x = size / 2.0;
    let center_y = size / 2.0 + size * 0.04;
    let outer = size * 0.48;
    let inner = size * 0.2;
    let mut path = String::new();
    for index in 0..10 {
        let radius = if index % 2 == 0 { outer } else { inner };
        let angle = index as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
        let x = center_x + radius * angle.cos();
        let y = center_y + radius * angle.sin();
        if index == 0 {
            path.push_str(&format!("M{x:.4} {y:.4}"));
        } else {
            path.push_str(&format!(" L{x:.4} {y:.4}"));
        }
    }
    path.push_str(" Z");
    path
}

fn mix_color(from: Color, to: Color, amount: f32) -> Color {
    let amount = if amount.is_finite() {
        amount.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let channel = |from: u8, to: u8| {
        (f32::from(from) + (f32::from(to) - f32::from(from)) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::rgba(
        channel(from.r, to.r),
        channel(from.g, to.g),
        channel(from.b, to.b),
        channel(from.a, to.a),
    )
}

/// Measures line widths with the host canvas when it exports
/// `myrust_measure_text`, and with the embedded face otherwise.
struct HostMeasurer<'a> {
    font: &'a FontFace,
}

impl TextMeasurer for HostMeasurer<'_> {
    fn measure(&self, text: &str, font_size: f32) -> TextMetrics {
        let font_size = font_size.max(1.0);
        let line_height = self.font.line_height(font_size);
        let lines = text.split('\n').collect::<Vec<_>>();
        let width = lines
            .iter()
            .map(|line| {
                host_text_width(line, font_size).unwrap_or_else(|| {
                    line.chars()
                        .map(|character| self.font.glyph_advance(character, font_size))
                        .sum()
                })
            })
            .fold(0.0_f32, f32::max);
        TextMetrics {
            width,
            height: lines.len().max(1) as f32 * line_height,
            line_height,
        }
    }
}

fn estimated_text_size(text: &str, font_size: f32, max_width: Option<f32>) -> Size {
    estimated_text_size_styled(text, font_size, max_width, false, 400, None)
}

/// Measures text as it will be drawn: at its own weight and family, so that
/// a bold or mono node is as wide as its glyphs, not as its 400-weight
/// system-font text would be.
fn estimated_text_size_styled(
    text: &str,
    font_size: f32,
    max_width: Option<f32>,
    mono: bool,
    weight: u16,
    family: Option<&str>,
) -> Size {
    // The line box is the styled face's, so a monospace run is measured
    // with the monospace face's ascent and descent, as MyGo lays it
    // (`internal/text/canvas_js.go` font).
    let line_height = measured_text_height_styled(font_size, mono);
    let mut widest = 0.0_f32;
    let mut lines = 0_usize;
    for line in text.split('\n') {
        let width = family
            .and_then(|family| {
                host_text_width_family(line, font_size, false, mono, weight, family)
            })
            .or_else(|| host_text_width_weighted(line, font_size, false, mono, weight))
            .or_else(|| host_text_width(line, font_size))
            .unwrap_or_else(|| {
                default_font()
                    .map(|font| {
                        line.chars()
                            .map(|character| font.glyph_advance(character, font_size))
                            .sum()
                    })
                    .unwrap_or_else(|| line.chars().count() as f32 * font_size * 0.5)
            });
        let width = match max_width {
            Some(max) if max > 0.0 => width.min(max),
            _ => width,
        };
        widest = widest.max(width);
        lines += 1;
    }
    Size::new(widest, lines.max(1) as f32 * line_height)
}

/// The hue of a name's FNV-1a hash, as MyGo picks an avatar's color
/// (`ui/indicators.go` coreAvatar).
fn fnv1a_hue(name: &str) -> f64 {
    let mut hash: u32 = 2_166_136_261;
    for byte in name.bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(16_777_619);
    }
    f64::from(hash % 360)
}

/// An HSL color at a hue, saturation and lightness, as MyGo's hslColor.
fn hsl_color(hue: f64, saturation: f64, lightness: f64) -> Color {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = lightness - c / 2.0;
    let (r, g, b) = if hue < 60.0 {
        (c, x, 0.0)
    } else if hue < 120.0 {
        (x, c, 0.0)
    } else if hue < 180.0 {
        (0.0, c, x)
    } else if hue < 240.0 {
        (0.0, x, c)
    } else if hue < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let channel = |value: f64| ((value + m) * 255.0) as u8;
    Color::rgb(channel(r), channel(g), channel(b))
}

/// The first letters of the first and last words of a name, uppercased, as
/// MyGo's initials (`ui/indicators.go`).
fn avatar_initials(name: &str) -> String {
    let mut first: Option<char> = None;
    let mut last: Option<char> = None;
    let mut in_word = false;
    for character in name.chars() {
        let letter = character.is_alphanumeric();
        if letter && !in_word {
            if first.is_none() {
                first = Some(character.to_uppercase().next().unwrap_or(character));
            } else {
                last = Some(character.to_uppercase().next().unwrap_or(character));
            }
        }
        in_word = letter;
    }
    match (first, last) {
        (None, _) => String::new(),
        (Some(first), None) => first.to_string(),
        (Some(first), Some(last)) => format!("{first}{last}"),
    }
}

/// Cuts `text` to what fits `max_width`, ending it with an ellipsis, as
/// MyGo's SingleLine with its default ellipsis does.
fn truncate_with_ellipsis(text: &str, font_size: f32, max_width: f32) -> String {
    if measure_text_width(text, font_size) <= max_width {
        return text.to_string();
    }
    let ellipsis = "\u{2026}";
    let ellipsis_width = measure_text_width(ellipsis, font_size);
    let mut kept = String::new();
    for character in text.chars() {
        let mut candidate = kept.clone();
        candidate.push(character);
        if measure_text_width(&candidate, font_size) + ellipsis_width > max_width {
            break;
        }
        kept = candidate;
    }
    kept.push_str(ellipsis);
    kept
}

pub fn measured_text_height(font_size: f32) -> f32 {
    measured_text_height_styled(font_size, false)
}

/// Returns the line box height of the styled font: MyGo lays a line as tall
/// as the canvas font's ascent and descent (`internal/text/canvas_js.go`
/// font), which the monospace family makes a pixel shorter than the sans
/// one.
pub fn measured_text_height_styled(font_size: f32, mono: bool) -> f32 {
    if let (Some(ascent), Some(descent)) = (
        host_text_ascent(font_size, false, mono, false),
        host_text_descent(font_size, false, mono, false),
    ) {
        return ascent + descent;
    }
    if let Some(font) = default_font() {
        font.line_height(font_size)
    } else {
        font_size * 1.25
    }
}

pub(crate) fn measure_text_width(text: &str, font_size: f32) -> f32 {
    if let Some(font) = default_font() {
        if let Some(width) = host_text_width(text, font_size) {
            return width;
        }
        text.chars()
            .map(|character| font.glyph_advance(character, font_size))
            .sum()
    } else {
        text.chars()
            .map(|character| ApproximateMeasurer::char_width(character, font_size))
            .sum()
    }
}

fn focus_ring(x: f32, y: f32, width: f32, height: f32, color: Color) -> [DrawCommand; 4] {
    [
        DrawCommand::Line {
            x1: x,
            y1: y,
            x2: x + width,
            y2: y,
            width: 2.0,
            color,
        },
        DrawCommand::Line {
            x1: x + width,
            y1: y,
            x2: x + width,
            y2: y + height,
            width: 2.0,
            color,
        },
        DrawCommand::Line {
            x1: x + width,
            y1: y + height,
            x2: x,
            y2: y + height,
            width: 2.0,
            color,
        },
        DrawCommand::Line {
            x1: x,
            y1: y + height,
            x2: x,
            y2: y,
            width: 2.0,
            color,
        },
    ]
}


fn line_spans(value: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    for (index, byte) in value.bytes().enumerate() {
        if byte == b'\n' {
            spans.push((start, index));
            start = index + 1;
        }
    }
    spans.push((start, value.len()));
    spans
}

fn line_index(spans: &[(usize, usize)], cursor: usize) -> usize {
    spans
        .iter()
        .position(|(start, end)| cursor >= *start && cursor <= *end)
        .unwrap_or(spans.len().saturating_sub(1))
}

fn byte_at_column(value: &str, start: usize, end: usize, column: usize) -> usize {
    value[start..end]
        .char_indices()
        .nth(column)
        .map(|(offset, _)| start + offset)
        .unwrap_or(end)
}

fn previous_char_boundary(value: &str, cursor: usize) -> Option<usize> {
    if cursor == 0 {
        None
    } else {
        value[..cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
    }
}

fn next_char_boundary(value: &str, cursor: usize) -> Option<usize> {
    value[cursor..]
        .char_indices()
        .nth(1)
        .map(|(offset, _)| cursor + offset)
        .or_else(|| (cursor < value.len()).then_some(value.len()))
}


fn month_name(month: u32) -> &'static str {
    [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ][(month.saturating_sub(1) as usize).min(11)]
}

fn normalize_size(size: Size) -> Size {
    Size::new(
        if size.width.is_finite() {
            size.width.max(1.0)
        } else {
            1.0
        },
        if size.height.is_finite() {
            size.height.max(1.0)
        } else {
            1.0
        },
    )
}

fn detect_reduce_motion() -> bool {
    if std::env::var("MYRUST_REDUCE_MOTION")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
        .unwrap_or(false)
    {
        return true;
    }
    false
}

fn sidebar_item_id<'a>(sections: &'a [SidebarSection<'_>], target: usize) -> Option<&'a str> {
    let mut flat = 0;
    for section in sections {
        for item in section.items {
            if flat == target {
                return Some(item.id);
            }
            flat += 1;
        }
    }
    None
}

/// How an image fills its box, as MyGo's Fit (`ui/paint.go` fitIn).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageFit {
    /// Stretch to the box, ignoring the picture's shape.
    #[default]
    Fill,
    /// Fit the whole picture inside the box, leaving room around it.
    Contain,
    /// Cover the box with the picture, cropping what falls outside.
    Cover,
    /// Contain, but never scale the picture up.
    ScaleDown,
    /// The picture's own size, cropped by the box.
    NaturalSize,
}

/// The face of an image: how it fills its box, its corner radius and
/// whether it draws in gray, as MyGo's Image takes Fit, Radius and
/// Grayscale.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImageStyle {
    /// How the picture fills the box.
    pub fit: ImageFit,
    /// Corner radius of the box.
    pub radius: f32,
    /// Whether the picture draws in gray.
    pub grayscale: bool,
}

/// The part of an image's source that shows, as fractions of it.
#[derive(Debug, Clone, Copy)]
struct ImageRegion {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Default for ImageRegion {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        }
    }
}

/// The rounded box an image's cells are masked by, in the image's own
/// coordinates: MyGo rounds the rect the picture is placed in
/// (`ui/paint.go` drawBitmap's `p.radii(radius)` on `dst`).
#[derive(Debug, Clone, Copy, Default)]
struct ImageMask {
    /// Corner radii of the placed rect.
    radius: Radius,
    /// Where the scaler's own origin sits inside the placed rect.
    origin_x: f32,
    /// Where the scaler's own origin sits inside the placed rect.
    origin_y: f32,
    /// Width of the placed rect.
    width: f32,
    /// Height of the placed rect.
    height: f32,
}

impl ImageMask {
    /// Returns how much of one cell falls inside the rounded rect, from 0
    /// for none to 1 for all.
    ///
    /// A corner's edge is a quarter circle, so a cell that straddles it
    /// covers a fraction no single distance tells: sampling a 4×4 grid over
    /// the cell and counting the points inside estimates that area the way
    /// MyGo's GPU shader antialiases the same edge.
    fn coverage(&self, x: f32, y: f32, cell_width: f32, cell_height: f32) -> f32 {
        if self.radius == Radius::default() || self.width <= 0.0 || self.height <= 0.0 {
            return 1.0;
        }
        // The cell in the placed rect's own coordinates.
        let left = x + self.origin_x;
        let top = y + self.origin_y;
        let right = left + cell_width;
        let bottom = top + cell_height;
        let limit = self.width.min(self.height) / 2.0;
        let corners = [
            (self.radius.top_left.clamp(0.0, limit), false, false),
            (self.radius.top_right.clamp(0.0, limit), true, false),
            (self.radius.bottom_right.clamp(0.0, limit), true, true),
            (self.radius.bottom_left.clamp(0.0, limit), false, true),
        ];
        // A corner only reaches into the cell when it straddles that
        // corner's quadrant.
        let mut straddled: Vec<(f32, f32, f32, bool, bool)> = Vec::new();
        for (corner_radius, right_side, bottom_side) in corners {
            if corner_radius <= 0.0 {
                continue;
            }
            let center_x = if right_side {
                self.width - corner_radius
            } else {
                corner_radius
            };
            let center_y = if bottom_side {
                self.height - corner_radius
            } else {
                corner_radius
            };
            let in_corner = if right_side {
                left > center_x
            } else {
                right < center_x
            } && if bottom_side {
                top > center_y
            } else {
                bottom < center_y
            };
            if in_corner {
                straddled.push((center_x, center_y, corner_radius, right_side, bottom_side));
            }
        }
        if straddled.is_empty() {
            return 1.0;
        }
        const STEPS: usize = 4;
        let mut inside = 0;
        for row in 0..STEPS {
            let sample_y = top + cell_height * (row as f32 + 0.5) / STEPS as f32;
            for column in 0..STEPS {
                let sample_x = left + cell_width * (column as f32 + 0.5) / STEPS as f32;
                let covered = straddled
                    .iter()
                    .all(|&(center_x, center_y, radius, right_side, bottom_side)| {
                        let dx = sample_x - center_x;
                        let dy = sample_y - center_y;
                        // The arc only cuts the corner's own quadrant; the
                        // rest of the rect's sides are straight and cover
                        // the point. A circle's four centers coincide at the
                        // middle, so the quadrant comes from the corner's
                        // side, not from where its center falls.
                        let towards_x = if right_side { dx > 0.0 } else { dx < 0.0 };
                        let towards_y = if bottom_side { dy > 0.0 } else { dy < 0.0 };
                        !(towards_x && towards_y) || dx * dx + dy * dy <= radius * radius
                    });
                if covered {
                    inside += 1;
                }
            }
        }
        inside as f32 / (STEPS * STEPS) as f32
    }
}

fn scaled_rgba_commands(
    data: ImageData<'_>,
    commands: &mut Vec<DrawCommand>,
    target_width: f32,
    target_height: f32,
    circular_mask: bool,
    scale: f32,
) -> (usize, usize) {
    // A circular mask rounds the whole box by half its width.
    let mask = if circular_mask {
        ImageMask {
            radius: Radius::all(target_width.min(target_height) / 2.0),
            origin_x: 0.0,
            origin_y: 0.0,
            width: target_width,
            height: target_height,
        }
    } else {
        ImageMask::default()
    };
    scaled_rgba_commands_with(
        data,
        commands,
        target_width,
        target_height,
        mask,
        ImageRegion::default(),
        false,
        scale,
    )
}

#[allow(clippy::too_many_arguments)]
/// Samples an RGBA picture at a fractional position, interpolating between
/// the four pixels around it, as a GPU texture sampler does.
fn bilinear_sample(
    pixels: &[u8],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
) -> (u8, u8, u8) {
    if width == 0 || height == 0 {
        return (0, 0, 0);
    }
    // Pixel centers sit at half-integers, so the sample moves back half a
    // pixel before it is split into a cell and a weight within it.
    let sample_x = (x - 0.5).max(0.0);
    let sample_y = (y - 0.5).max(0.0);
    let x0 = (sample_x.floor() as usize).min(width - 1);
    let y0 = (sample_y.floor() as usize).min(height - 1);
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = sample_x - sample_x.floor();
    let fy = sample_y - sample_y.floor();
    let texel = |px: usize, py: usize, channel: usize| -> f32 {
        f32::from(pixels[(py * width + px) * 4 + channel])
    };
    let mut out = [0_u8; 3];
    for (channel, slot) in out.iter_mut().enumerate() {
        let top = texel(x0, y0, channel) * (1.0 - fx) + texel(x1, y0, channel) * fx;
        let bottom = texel(x0, y1, channel) * (1.0 - fx) + texel(x1, y1, channel) * fx;
        *slot = (top * (1.0 - fy) + bottom * fy).round() as u8;
    }
    (out[0], out[1], out[2])
}

fn scaled_rgba_commands_with(
    data: ImageData<'_>,
    commands: &mut Vec<DrawCommand>,
    target_width: f32,
    target_height: f32,
    mask: ImageMask,
    region: ImageRegion,
    grayscale: bool,
    scale: f32,
) -> (usize, usize) {
    if !data.is_valid() {
        return (0, 0);
    }
    // MyGo samples the picture once per device pixel, as its shader runs
    // over the frame's pixels, not its DIPs.
    let scale = scale.max(1.0);
    let columns = ((target_width * scale).round() as usize).clamp(1, 1024);
    let rows = ((target_height * scale).round() as usize).clamp(1, 1024);
    let cell_width = target_width / columns as f32;
    let cell_height = target_height / rows as f32;
    let source_width = data.width as usize;
    let source_height = data.height as usize;
    for row in 0..rows {
        for column in 0..columns {
            let coverage = mask.coverage(
                column as f32 * cell_width,
                row as f32 * cell_height,
                cell_width,
                cell_height,
            );
            if coverage <= 0.0 {
                continue;
            }
            let sample_x = region.x * source_width as f32
                + (column as f32 + 0.5) / columns as f32 * region.width * source_width as f32;
            let sample_y = region.y * source_height as f32
                + (row as f32 + 0.5) / rows as f32 * region.height * source_height as f32;
            // MyGo draws an image as one GPU texture, whose sampler
            // interpolates between source pixels; sampling each cell's
            // center bilinearly keeps a scaled picture's gradients and
            // edges as smooth as that draw, where nearest sampling would
            // step them.
            let (mut red, mut green, mut blue) =
                bilinear_sample(data.pixels, source_width, source_height, sample_x, sample_y);
            if grayscale {
                let luma = (0.2126 * f32::from(red) + 0.7152 * f32::from(green) + 0.0722 * f32::from(blue))
                    .round() as u8;
                red = luma;
                green = luma;
                blue = luma;
            }
            let nearest_x = (sample_x as usize).min(source_width - 1);
            let nearest_y = (sample_y as usize).min(source_height - 1);
            let alpha_offset = (nearest_y * source_width + nearest_x) * 4 + 3;
            let alpha = (f32::from(data.pixels[alpha_offset]) * coverage).round() as u8;
            let color = Color::rgba(red, green, blue, alpha);
            commands.push(DrawCommand::Rect {
                x: column as f32 * cell_width,
                y: row as f32 * cell_height,
                width: cell_width,
                height: cell_height,
                color,
            });
        }
    }
    (columns, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct SwitchProbe {
        checked: bool,
        changed: bool,
    }

    #[derive(Default)]
    struct RadioProbe {
        selected: usize,
    }

    #[derive(Default)]
    struct SelectProbe {
        selected: usize,
        open: bool,
        reported_selected: usize,
        changed: bool,
        focused: bool,
    }

    #[derive(Default)]
    struct TabsProbe {
        selected: usize,
    }

    #[derive(Default)]
    struct CollapsibleProbe {
        open: bool,
    }

    #[derive(Default)]
    struct OverlayBlockProbe {
        pressed: bool,
        clicks: u32,
    }

    #[derive(Default)]
    struct OverlayFocusProbe {
        base: bool,
        inner: bool,
    }

    #[test]
    fn builders_register_layout_and_position_commands() {
        let mut context = Context::new(Size::new(200.0, 120.0));
        context.begin_frame(Size::new(200.0, 120.0));
        column(&mut context).padding(10.0).gap(4.0).children(|c| {
            text(c, "first").font_size(12.0);
            text(c, "second").font_size(12.0);
        });
        let frame = context.produce().unwrap();
        assert_eq!(frame.layout.len(), 3);
        assert_eq!(frame.display.commands.len(), 2);
        let texts = frame
            .display
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, x, y, .. } => Some((text.as_str(), *x, *y)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(texts[0].0, "first");
        assert_eq!(texts[1].0, "second");
        assert!(texts[1].2 > texts[0].2);
    }

    #[test]
    fn explicit_id_is_used_for_button_regions() {
        let mut context = Context::new(Size::new(200.0, 120.0));
        context.begin_frame(Size::new(200.0, 120.0));
        context.id("save").button("Save");
        let frame = context.produce().unwrap();
        assert_eq!(frame.interactions[0].button_id, "save");
    }

    #[test]
    fn text_builder_updates_command_and_size() {
        let mut context = Context::new(Size::new(200.0, 120.0));
        context.begin_frame(Size::new(200.0, 120.0));
        text(&mut context, "hello")
            .font_size(20.0)
            .bold()
            .color(Color::rgb(1, 2, 3));
        let frame = context.produce().unwrap();
        let DrawCommand::Text {
            font_size,
            weight,
            color,
            ..
        } = frame.display.commands[0]
        else {
            panic!("expected text command");
        };
        assert_eq!(font_size, 20.0);
        assert_eq!(weight, 700);
        assert_eq!(color, Color::rgb(1, 2, 3));
    }

    #[test]
    fn nested_containers_resolve_absolute_child_positions() {
        let mut context = Context::new(Size::new(300.0, 200.0));
        context.begin_frame(Size::new(300.0, 200.0));
        column(&mut context).padding(10.0).children(|c| {
            row(c).padding(5.0).gap(8.0).children(|c| {
                text(c, "left");
                text(c, "right");
            });
        });
        let frame = context.produce().unwrap();
        let texts = frame
            .display
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, x, y, .. } => Some((text.as_str(), *x, *y)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].0, "left");
        assert_eq!(texts[1].0, "right");
        assert_eq!(texts[0].1, 15.0);
        assert_eq!(texts[0].2, 15.0);
        assert!(texts[1].1 > texts[0].1);
    }

    #[test]
    fn button_pressed_state_uses_previous_region_and_pointer_down() {
        let mut context = Context::new(Size::new(200.0, 120.0));
        context.begin_frame(Size::new(200.0, 120.0));
        context.id("press").button("Press");
        let first = context.produce().unwrap();
        context.end_frame();
        let region = &first.interactions[0];
        let point = Point::new(
            region.bounds.x + region.bounds.width / 2.0,
            region.bounds.y + region.bounds.height / 2.0,
        );

        context.begin_frame(Size::new(200.0, 120.0));
        context.set_input(UiEvent::PointerPressed {
            x: point.x,
            y: point.y,
            button: crate::event::PointerButton::Left,
        });
        let state = context.id("press").button("Press");
        assert!(state.pressed);
        assert!(!state.clicked);
    }

    #[test]
    fn hover_uses_previous_frame_region() {
        let mut context = Context::new(Size::new(200.0, 120.0));
        context.begin_frame(Size::new(200.0, 120.0));
        context.id("hover-me").button("Hover");
        let frame = context.produce().unwrap();
        context.end_frame();
        let region = &frame.interactions[0];
        let x = region.bounds.x + region.bounds.width / 2.0;
        let y = region.bounds.y + region.bounds.height / 2.0;

        context.begin_frame(Size::new(200.0, 120.0));
        context.set_input(UiEvent::PointerMoved { x, y });
        assert!(context.hover("hover-me"));
        context.button("Hover");
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(200.0, 120.0));
        context.set_input(UiEvent::PointerMoved {
            x: x + 1000.0,
            y: y + 1000.0,
        });
        assert!(!context.hover("hover-me"));
    }

    fn build_three_buttons(context: &mut Context) -> Vec<ButtonState> {
        vec![
            context.id("first").button("First"),
            context.id("second").button("Second"),
            context.id("third").button("Third"),
        ]
    }

    #[test]
    fn tab_and_shift_tab_cycle_focus_and_enter_activates() {
        let mut context = Context::new(Size::new(300.0, 200.0));
        context.begin_frame(Size::new(300.0, 200.0));
        build_three_buttons(&mut context);
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(300.0, 200.0));
        context.set_input(UiEvent::Key { key: Key::Tab });
        let first = build_three_buttons(&mut context);
        assert!(first[0].focused);
        let focus_frame = context.produce().unwrap();
        assert!(
            focus_frame
                .display
                .commands
                .iter()
                .filter(|command| matches!(command, DrawCommand::Line { .. }))
                .count()
                >= 4
        );
        context.end_frame();

        context.begin_frame(Size::new(300.0, 200.0));
        context.set_input(UiEvent::Key { key: Key::Tab });
        let second = build_three_buttons(&mut context);
        assert!(second[1].focused);
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(300.0, 200.0));
        context.set_input(UiEvent::Key { key: Key::Tab });
        let third = build_three_buttons(&mut context);
        assert!(third[2].focused);
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(300.0, 200.0));
        context.set_input(UiEvent::Key { key: Key::ShiftTab });
        let reverse = build_three_buttons(&mut context);
        assert!(reverse[1].focused);
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(300.0, 200.0));
        context.set_input(UiEvent::Key { key: Key::ShiftTab });
        let first_again = build_three_buttons(&mut context);
        assert!(first_again[0].focused);
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(300.0, 200.0));
        context.set_input(UiEvent::Key { key: Key::Enter });
        let activated = build_three_buttons(&mut context);
        assert!(activated[0].clicked);
    }

    #[test]
    fn focusable_builder_adds_non_button_widget_to_tab_order() {
        let mut context = Context::new(Size::new(200.0, 120.0));
        context.begin_frame(Size::new(200.0, 120.0));
        context.id("focusable-text");
        text(&mut context, "Focusable").focusable();
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(200.0, 120.0));
        context.set_input(UiEvent::Key { key: Key::Tab });
        context.id("focusable-text");
        text(&mut context, "Focusable").focusable();
        assert!(context.is_focused());
        assert!(context.focus("focusable-text"));
    }

    #[test]
    fn text_field_supports_editing_keys_and_cursor_motion() {
        let mut context = Context::new(Size::new(300.0, 120.0));
        let mut value = String::new();
        context.begin_frame(Size::new(300.0, 120.0));
        context.request_focus("editor");
        let state = context.text_field("editor", &mut value);
        assert!(state.focused);
        context.produce().unwrap();
        context.end_frame();

        for (key, expected, expected_cursor, expected_changed) in [
            (UiEvent::TextInput { text: "abc".into() }, "abc", 3, true),
            (
                UiEvent::Key {
                    key: Key::Backspace,
                },
                "ab",
                2,
                true,
            ),
            (UiEvent::Key { key: Key::Home }, "ab", 0, false),
            (UiEvent::Key { key: Key::Delete }, "b", 0, true),
        ] {
            context.begin_frame(Size::new(300.0, 120.0));
            context.set_input(key);
            let state = context.text_field("editor", &mut value);
            assert_eq!(value, expected);
            assert_eq!(state.cursor, expected_cursor);
            assert_eq!(state.changed, expected_changed);
            context.produce().unwrap();
            context.end_frame();
        }
    }


    #[test]
    fn text_area_inserts_newlines_and_moves_between_lines() {
        let mut context = Context::new(Size::new(300.0, 160.0));
        let mut value = String::new();
        context.begin_frame(Size::new(300.0, 160.0));
        context.request_focus("area");
        let state = context.text_area("area", &mut value, 3);
        assert!(state.focused);
        context.produce().unwrap();
        context.end_frame();

        for (event, expected, expected_cursor) in [
            (UiEvent::TextInput { text: "ab".into() }, "ab", 2),
            (UiEvent::Key { key: Key::Enter }, "ab\n", 3),
            (UiEvent::TextInput { text: "cd".into() }, "ab\ncd", 5),
            (UiEvent::Key { key: Key::ArrowUp }, "ab\ncd", 2),
            (UiEvent::Key { key: Key::End }, "ab\ncd", 2),
            (UiEvent::Key { key: Key::ArrowDown }, "ab\ncd", 5),
        ] {
            context.begin_frame(Size::new(300.0, 160.0));
            context.set_input(event);
            let state = context.text_area("area", &mut value, 3);
            assert_eq!(value, expected);
            assert_eq!(state.cursor, expected_cursor);
            context.produce().unwrap();
            context.end_frame();
        }
    }

    #[test]
    fn password_field_keeps_value_and_renders_bullets() {
        let mut context = Context::new(Size::new(300.0, 120.0));
        let mut value = "secret".to_string();
        context.begin_frame(Size::new(300.0, 120.0));
        context.text_field_password("password", &mut value);
        let has_bullets = context.nodes.values().any(|node| {
            node.commands.iter().any(|command| {
                matches!(command, DrawCommand::Text { text, .. } if text == "••••••")
            })
        });
        assert!(has_bullets);
        assert_eq!(value, "secret");
        context.produce().unwrap();
        context.end_frame();
    }

    #[test]
    fn checkbox_mouse_and_space_toggle_state() {
        let mut context = Context::new(Size::new(300.0, 120.0));
        let mut checked = true;
        context.begin_frame(Size::new(300.0, 120.0));
        context.checkbox("toggle", &mut checked);
        let frame = context.produce().unwrap();
        context.end_frame();
        let region = &frame.interactions[0];
        let x = region.bounds.x + 10.0;
        let y = region.bounds.y + 10.0;

        context.begin_frame(Size::new(300.0, 120.0));
        context.set_input(UiEvent::PointerPressed {
            x,
            y,
            button: crate::event::PointerButton::Left,
        });
        context.checkbox("toggle", &mut checked);
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(300.0, 120.0));
        context.set_input(UiEvent::PointerReleased {
            x,
            y,
            button: crate::event::PointerButton::Left,
        });
        let clicked = context.checkbox("toggle", &mut checked);
        assert!(clicked.changed);
        assert!(!checked);
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(300.0, 120.0));
        context.set_input(UiEvent::Key { key: Key::Space });
        let toggled = context.checkbox("toggle", &mut checked);
        assert!(toggled.changed);
        assert!(checked);
    }

    #[test]
    fn slider_drag_maps_to_midpoint_and_arrow_step_works() {
        let mut context = Context::new(Size::new(300.0, 120.0));
        let mut value = 0.0;
        context.begin_frame(Size::new(300.0, 120.0));
        context.slider("level", &mut value, 0.0, 100.0);
        let frame = context.produce().unwrap();
        context.end_frame();
        let region = &frame.interactions[0];
        let x = region.bounds.x + region.bounds.width / 2.0;
        let y = region.bounds.y + region.bounds.height / 2.0;

        context.begin_frame(Size::new(300.0, 120.0));
        context.set_input(UiEvent::PointerPressed {
            x,
            y,
            button: crate::event::PointerButton::Left,
        });
        context.slider("level", &mut value, 0.0, 100.0);
        assert!((value - 50.0).abs() <= 5.0, "midpoint value was {value}");
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(300.0, 120.0));
        context.set_input(UiEvent::PointerReleased {
            x,
            y,
            button: crate::event::PointerButton::Left,
        });
        context.slider("level", &mut value, 0.0, 100.0);
        context.produce().unwrap();
        context.end_frame();
        let midpoint = value;

        context.begin_frame(Size::new(300.0, 120.0));
        context.set_input(UiEvent::Key {
            key: Key::ArrowRight,
        });
        let state = context.slider("level", &mut value, 0.0, 100.0);
        assert!(state.changed);
        assert!((value - (midpoint + 1.0)).abs() < 0.01);
    }

    #[test]
    fn scroll_view_clamps_and_emits_balanced_scissor() {
        let mut context = Context::new(Size::new(220.0, 160.0));
        context.begin_frame(Size::new(220.0, 160.0));
        let initial = context.scroll_view("scroll", 80.0, |c| {
            column(c).height(320.0);
        });
        assert_eq!(initial.max_scroll, 240.0);
        let frame = context.produce().unwrap();
        context.end_frame();
        let region = &frame.interactions[0];
        let x = region.bounds.x + 20.0;
        let y = region.bounds.y + 20.0;

        context.begin_frame(Size::new(220.0, 160.0));
        context.set_input(UiEvent::PointerMoved { x, y });
        context.set_input(UiEvent::Wheel {
            delta_x: 0.0,
            delta_y: 1000.0,
        });
        let scrolled = context.scroll_view("scroll", 80.0, |c| {
            column(c).height(320.0);
        });
        assert_eq!(scrolled.scroll, 240.0);
        assert!(scrolled.changed);
        let frame = context.produce().unwrap();
        let pushes = frame
            .display
            .commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::ScissorPush { .. }))
            .count();
        let pops = frame
            .display
            .commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::ScissorPop))
            .count();
        assert_eq!(pushes, 1);
        assert_eq!(pops, 1);
    }

    #[test]
    fn scroll_view_scrollbar_drag_updates_offset() {
        let mut context = Context::new(Size::new(220.0, 160.0));
        context.begin_frame(Size::new(220.0, 160.0));
        context.scroll_view("scroll-drag", 80.0, |c| {
            column(c).height(320.0);
        });
        let frame = context.produce().unwrap();
        context.end_frame();
        let region = &frame.interactions[0];
        let x = region.bounds.right() - 4.0;
        let y = region.bounds.y + 40.0;

        context.begin_frame(Size::new(220.0, 160.0));
        context.set_input(UiEvent::PointerPressed {
            x,
            y,
            button: crate::event::PointerButton::Left,
        });
        let scroll = context.scroll_view("scroll-drag", 80.0, |c| {
            column(c).height(320.0);
        });
        assert!(scroll.dragging);
        assert!(scroll.scroll > 0.0);
        assert!(scroll.scroll <= scroll.max_scroll);
    }

    #[test]
    fn animation_advances_with_elapsed_time_and_reports_active_frames() {
        let mut context = Context::new(Size::new(100.0, 80.0));
        assert_eq!(context.animate("hover", 0.0), 0.0);
        let started = context.animate("hover", 1.0);
        assert!(started <= 0.001, "initial animated value was {started}");
        assert!(context.needs_animation_frame());

        context.advance_frame_time_for_test(Duration::from_millis(60));
        let midpoint = context.animate("hover", 1.0);
        assert!(
            midpoint > 0.5 && midpoint < 1.0,
            "midpoint animated value was {midpoint}"
        );
        assert!(context.needs_animation_frame());

        context.advance_frame_time_for_test(Duration::from_millis(60));
        let finished = context.animate("hover", 1.0);
        assert_eq!(finished, 1.0);
        assert!(!context.needs_animation_frame());
    }

    #[test]
    fn view_frame_reports_animation_frames_until_convergence() {
        let mut context = Context::new(Size::new(160.0, 100.0));
        assert_eq!(context.animate("surface", 0.0), 0.0);
        column(&mut context).children(|context| {
            text(context, "animated");
        });
        let _ = context.animate("surface", 1.0);
        let active_frame = context.produce().unwrap();
        assert!(active_frame.needs_animation_frame);

        context.advance_frame_time_for_test(Duration::from_millis(150));
        assert_eq!(context.animate("surface", 1.0), 1.0);
        let settled_frame = context.produce().unwrap();
        assert!(!settled_frame.needs_animation_frame);
    }

    #[test]
    fn reduce_motion_jumps_directly_to_the_target() {
        let mut context = Context::new(Size::new(100.0, 80.0));
        context.set_reduce_motion_for_test(true);
        assert_eq!(context.animate("hover", 0.0), 0.0);
        assert_eq!(context.animate("hover", 1.0), 1.0);
        assert!(!context.needs_animation_frame());
    }

    #[test]
    fn button_hover_transition_changes_background_after_elapsed_time() {
        let mut theme = Theme::dark();
        theme.surface = Color::rgb(12, 20, 30);
        theme.accent = Color::rgb(40, 200, 220);
        let mut context = Context::new(Size::new(220.0, 100.0));
        context.set_theme(theme.clone());
        context.set_frame_time_for_test(Instant::now());

        context.begin_frame(Size::new(220.0, 100.0));
        context.id("animated-hover").button("Hover");
        let initial = context.produce().unwrap();
        context.end_frame();
        let region = &initial.interactions[0];
        let x = region.bounds.x + region.bounds.width / 2.0;
        let y = region.bounds.y + region.bounds.height / 2.0;

        context.begin_frame(Size::new(220.0, 100.0));
        context.set_input(UiEvent::PointerMoved { x, y });
        context.id("animated-hover").button("Hover");
        assert!(context.needs_animation_frame());
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(220.0, 100.0));
        context.advance_frame_time_for_test(Duration::from_millis(60));
        context.id("animated-hover").button("Hover");
        let frame = context.produce().unwrap();
        let background = frame
            .display
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::RoundedRect { x, color, .. } if *x == 1.0 => Some(*color),
                _ => None,
            })
            .expect("button background");
        let midpoint = Animation::default().value(0.5);
        assert_eq!(
            background,
            mix_color(theme.surface, theme.surface_hover, midpoint),
            "hover animation should interpolate toward the themed surface hover"
        );
    }

    #[test]
    fn accessibility_nodes_capture_roles_values_and_focus() {
        let find_node = |frame: &super::super::native::ViewFrame, label: &str| {
            frame
                .access_nodes
                .iter()
                .find(|node| node.label == label)
                .unwrap_or_else(|| panic!("missing access node {label}"))
                .clone()
        };

        let mut button_context = Context::new(Size::new(180.0, 90.0));
        button_context.begin_frame(Size::new(180.0, 90.0));
        button_context.request_focus("save-action");
        button_context.id("save-action").button("Save");
        let button_frame = button_context.produce().unwrap();
        let button_node = find_node(&button_frame, "Save");
        assert_eq!(button_node.role, Role::Button);
        assert_eq!(button_node.value, None);
        assert!(button_node.focused);

        let mut checkbox_context = Context::new(Size::new(180.0, 90.0));
        checkbox_context.begin_frame(Size::new(180.0, 90.0));
        checkbox_context.request_focus("check");
        let mut checked = true;
        checkbox_context.checkbox("check", &mut checked);
        let checkbox_frame = checkbox_context.produce().unwrap();
        let checkbox_node = find_node(&checkbox_frame, "checkbox");
        assert_eq!(checkbox_node.role, Role::CheckBox);
        assert_eq!(checkbox_node.value.as_deref(), Some("checked"));
        assert!(checkbox_node.focused);

        let mut slider_context = Context::new(Size::new(220.0, 90.0));
        slider_context.begin_frame(Size::new(220.0, 90.0));
        slider_context.request_focus("level");
        let mut value = 50.0;
        slider_context.slider("level", &mut value, 0.0, 100.0);
        let slider_frame = slider_context.produce().unwrap();
        let slider_node = find_node(&slider_frame, "slider");
        assert_eq!(slider_node.role, Role::Slider);
        assert_eq!(slider_node.value.as_deref(), Some("50.00"));
        assert!(slider_node.focused);

        let mut text_context = Context::new(Size::new(220.0, 90.0));
        text_context.begin_frame(Size::new(220.0, 90.0));
        text_context.request_focus("name");
        let mut text_value = "hello".to_string();
        text_context.text_field("name", &mut text_value);
        let text_frame = text_context.produce().unwrap();
        let text_node = find_node(&text_frame, "text field");
        assert_eq!(text_node.role, Role::TextField);
        assert_eq!(text_node.value.as_deref(), Some("hello"));
        assert!(text_node.focused);
    }

    #[test]
    fn button_draw_commands_derive_colors_from_the_theme() {
        let mut theme = Theme::dark();
        theme.surface = Color::rgb(7, 9, 11);
        theme.foreground = Color::rgb(201, 203, 205);
        theme.accent = Color::rgb(11, 13, 17);

        let mut context = Context::new(Size::new(180.0, 90.0));
        context.begin_frame(Size::new(180.0, 90.0));
        context.set_theme(theme.clone());
        context.request_focus("themed-button");
        context.id("themed-button").button("Themed");
        let frame = context.produce().unwrap();

        let background = frame
            .display
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::RoundedRect { x, color, .. } if *x == 1.0 => Some(*color),
                _ => None,
            })
            .expect("button background");
        assert_eq!(background, theme.surface);
        assert!(frame.display.commands.iter().any(|command| {
            matches!(command, DrawCommand::Line { color, .. } if *color == theme.accent)
        }));
    }

    fn render_frame(
        context: &mut Context,
        size: Size,
        input: Option<UiEvent>,
        build: &mut impl FnMut(&mut Context),
    ) -> super::super::native::ViewFrame {
        context.begin_frame(size);
        if let Some(event) = input {
            context.set_input(event);
        }
        build(context);
        let frame = context.produce().unwrap();
        context.end_frame();
        frame
    }

    fn center_of(frame: &super::super::native::ViewFrame, id: &str) -> Point {
        let region = frame
            .interactions
            .iter()
            .find(|region| region.button_id == id)
            .unwrap_or_else(|| panic!("missing interaction region {id}"));
        Point::new(
            region.bounds.x + region.bounds.width / 2.0,
            region.bounds.y + region.bounds.height / 2.0,
        )
    }

    fn click(
        context: &mut Context,
        size: Size,
        id: &str,
        build: &mut impl FnMut(&mut Context),
    ) -> super::super::native::ViewFrame {
        let initial = render_frame(context, size, None, build);
        let point = center_of(&initial, id);
        render_frame(
            context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: crate::event::PointerButton::Left,
            }),
            build,
        );
        render_frame(
            context,
            size,
            Some(UiEvent::PointerReleased {
                x: point.x,
                y: point.y,
                button: crate::event::PointerButton::Left,
            }),
            build,
        )
    }

    #[test]
    fn overlay_draws_last_and_exposes_its_own_region() {
        let size = Size::new(320.0, 220.0);
        let mut context = Context::new(size);
        let mut state = OverlayState::new(true);
        let frame = render_frame(&mut context, size, None, &mut |context| {
            context.overlay(
                "popup-z",
                &mut state,
                Rect::new(20.0, 40.0, 180.0, 30.0),
                |context| {
                    context.id("inside").button("Inside");
                },
            );
            context.id("under").button("Under");
        });
        assert_eq!(
            frame
                .interactions
                .last()
                .expect("overlay interaction")
                .button_id,
            "inside"
        );
        let under_text = frame
            .display
            .commands
            .iter()
            .position(
                |command| matches!(command, DrawCommand::Text { text, .. } if text == "Under"),
            )
            .expect("base text");
        let inside_text = frame
            .display
            .commands
            .iter()
            .position(
                |command| matches!(command, DrawCommand::Text { text, .. } if text == "Inside"),
            )
            .expect("overlay text");
        assert!(inside_text > under_text, "overlay must paint above content");
        assert!(state.open);
    }

    #[test]
    fn overlay_blocks_lower_layer_and_outside_click_dismisses() {
        let size = Size::new(320.0, 220.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(OverlayState::new(true)));
        let probe = Rc::new(RefCell::new(OverlayBlockProbe::default()));
        let view_state = Rc::clone(&state);
        let view_probe = Rc::clone(&probe);
        let mut build = move |context: &mut Context| {
            let mut probe = view_probe.borrow_mut();
            let button = context.id("under").button("Under");
            if button.clicked {
                probe.clicks += 1;
            }
            if button.pressed {
                probe.pressed = true;
            }
            let mut overlay = view_state.borrow_mut();
            let _ = context.overlay(
                "popup-block",
                &mut overlay,
                Rect::new(20.0, 80.0, 180.0, 30.0),
                |context| {
                    context.id("inside").button("Inside");
                },
            );
        };
        let first = render_frame(&mut context, size, None, &mut build);
        let under = center_of(&first, "under");
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: under.x,
                y: under.y,
                button: crate::event::PointerButton::Left,
            }),
            &mut build,
        );
        assert!(
            !probe.borrow().pressed,
            "background press must be suppressed"
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: under.x,
                y: under.y,
                button: crate::event::PointerButton::Left,
            }),
            &mut build,
        );
        assert_eq!(
            probe.borrow().clicks,
            0,
            "dismiss click must not reach the button"
        );
        assert!(
            !state.borrow().open,
            "release outside must close the overlay"
        );
    }

    #[test]
    fn overlay_traps_tab_focus_and_restores_it_after_escape() {
        let size = Size::new(320.0, 220.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(OverlayState::new(true)));
        let probe = Rc::new(RefCell::new(OverlayFocusProbe::default()));
        let view_state = Rc::clone(&state);
        let view_probe = Rc::clone(&probe);
        let first = Rc::new(RefCell::new(true));
        let view_first = Rc::clone(&first);
        let mut build = move |context: &mut Context| {
            if *view_first.borrow() {
                context.request_focus("base");
                *view_first.borrow_mut() = false;
            }
            let mut probe = view_probe.borrow_mut();
            probe.base = context.id("base").button("Base").focused;
            let mut overlay = view_state.borrow_mut();
            let _ = context.overlay(
                "popup-focus",
                &mut overlay,
                Rect::new(20.0, 80.0, 180.0, 30.0),
                |context| {
                    probe.inner = context.id("inner").button("Inner").focused;
                },
            );
        };
        render_frame(&mut context, size, None, &mut build);
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Tab }),
            &mut build,
        );
        assert!(probe.borrow().inner, "focus enters the popup on open");
        assert!(
            !probe.borrow().base,
            "focus must not stay behind the overlay"
        );

        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert!(!state.borrow().open, "Escape closes the overlay");
        render_frame(&mut context, size, None, &mut build);
        assert!(
            probe.borrow().base,
            "focus returns to the widget that opened it"
        );
    }

    #[test]
    fn switch_clicks_space_and_accessibility_value() {
        let size = Size::new(240.0, 120.0);
        let mut context = Context::new(size);
        let probe = Rc::new(RefCell::new(SwitchProbe::default()));
        let view_probe = Rc::clone(&probe);
        let mut build = move |context: &mut Context| {
            let mut probe = view_probe.borrow_mut();
            let state = context.switch("switch", &mut probe.checked);
            probe.changed = state.changed;
        };
        click(&mut context, size, "switch", &mut build);
        assert!(probe.borrow().changed);
        assert!(probe.borrow().checked);

        context.request_focus("switch");
        let frame = render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Space }),
            &mut build,
        );
        assert!(!probe.borrow().checked);
        let node = frame
            .access_nodes
            .iter()
            .find(|node| node.role == Role::Switch)
            .expect("switch access node");
        assert_eq!(node.value.as_deref(), Some("off"));
    }

    #[test]
    fn radio_group_click_and_arrow_keys_move_one_selection() {
        let size = Size::new(260.0, 160.0);
        let mut context = Context::new(size);
        let probe = Rc::new(RefCell::new(RadioProbe::default()));
        let view_probe = Rc::clone(&probe);
        let mut build = move |context: &mut Context| {
            let mut probe = view_probe.borrow_mut();
            context.radio_group("mode", &mut probe.selected, &["One", "Two", "Three"]);
        };
        click(&mut context, size, "mode:1", &mut build);
        assert_eq!(probe.borrow().selected, 1);
        let one_selected = render_frame(&mut context, size, None, &mut build);
        let selected_radios = one_selected
            .access_nodes
            .iter()
            .filter(|node| node.role == Role::Radio && node.value.as_deref() == Some("selected"))
            .count();
        assert_eq!(selected_radios, 1, "exactly one radio may be selected");
        click(&mut context, size, "mode:2", &mut build);
        assert_eq!(probe.borrow().selected, 2);
        let moved = render_frame(&mut context, size, None, &mut build);
        let selected_radios = moved
            .access_nodes
            .iter()
            .filter(|node| node.role == Role::Radio && node.value.as_deref() == Some("selected"))
            .count();
        assert_eq!(
            selected_radios, 1,
            "clicking another option deselects the old one"
        );
        click(&mut context, size, "mode:0", &mut build);
        assert_eq!(probe.borrow().selected, 0);

        context.request_focus("mode");
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowRight,
            }),
            &mut build,
        );
        assert_eq!(probe.borrow().selected, 1);
        let frame = render_frame(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowLeft,
            }),
            &mut build,
        );
        assert_eq!(probe.borrow().selected, 0);
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::RadioGroup && node.label == "mode"));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Radio && node.value.as_deref() == Some("selected")));
    }

    #[test]
    fn select_opens_overlays_selects_by_click_and_closes_on_escape() {
        let size = Size::new(320.0, 240.0);
        let mut context = Context::new(size);
        let probe = Rc::new(RefCell::new(SelectProbe::default()));
        let view_probe = Rc::clone(&probe);
        let mut build = move |context: &mut Context| {
            let mut probe = view_probe.borrow_mut();
            let state = context.select("pick", &mut probe.selected, &["Alpha", "Beta", "Gamma"]);
            probe.open = state.open;
            probe.reported_selected = state.selected;
            probe.changed = state.changed;
            probe.focused = state.focused;
        };
        click(&mut context, size, "pick", &mut build);
        assert!(
            probe.borrow().open,
            "clicking the trigger opens the drop-down"
        );
        click(&mut context, size, "pick:option:1", &mut build);
        assert_eq!(probe.borrow().selected, 1);
        assert_eq!(probe.borrow().reported_selected, 1);
        assert!(probe.borrow().changed);
        assert!(
            !probe.borrow().open,
            "choosing an option closes the drop-down"
        );

        render_frame(&mut context, size, None, &mut build);
        assert!(
            probe.borrow().focused,
            "closing restores focus to the trigger"
        );
        click(&mut context, size, "pick", &mut build);
        assert!(probe.borrow().open);
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert!(!probe.borrow().open, "Escape closes the drop-down");
    }

    #[test]
    fn select_keyboard_opens_highlights_and_commits() {
        let size = Size::new(320.0, 240.0);
        let mut context = Context::new(size);
        context.request_focus("kpick");
        let probe = Rc::new(RefCell::new(SelectProbe::default()));
        let view_probe = Rc::clone(&probe);
        let mut build = move |context: &mut Context| {
            let mut probe = view_probe.borrow_mut();
            context.select("kpick", &mut probe.selected, &["Alpha", "Beta", "Gamma"]);
        };
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        let frame = render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert_eq!(probe.borrow().selected, 1);
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Option && node.value.as_deref() == Some("selected")));
    }

    #[test]
    fn select_keyboard_escape_cancels_without_committing_the_highlight() {
        let size = Size::new(320.0, 240.0);
        let mut context = Context::new(size);
        context.request_focus("cancel-select");
        let probe = Rc::new(RefCell::new(SelectProbe {
            selected: 2,
            ..SelectProbe::default()
        }));
        let view_probe = Rc::clone(&probe);
        let mut build = move |context: &mut Context| {
            let mut probe = view_probe.borrow_mut();
            let state = context.select(
                "cancel-select",
                &mut probe.selected,
                &["Alpha", "Beta", "Gamma"],
            );
            probe.open = state.open;
            probe.changed = state.changed;
            probe.reported_selected = state.selected;
        };
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert!(probe.borrow().open, "Enter opens the focused select");
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::ArrowUp }),
            &mut build,
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert!(!probe.borrow().open, "Escape closes the open select");
        assert_eq!(
            probe.borrow().selected,
            2,
            "Escape must not commit the highlighted option"
        );
        assert!(!probe.borrow().changed);
    }

    #[test]
    fn tabs_click_and_arrow_keys_change_the_page_index() {
        let size = Size::new(320.0, 160.0);
        let mut context = Context::new(size);
        let probe = Rc::new(RefCell::new(TabsProbe::default()));
        let view_probe = Rc::clone(&probe);
        let mut build = move |context: &mut Context| {
            let mut probe = view_probe.borrow_mut();
            context.tabs("pages", &mut probe.selected, &["One", "Two", "Three"]);
        };
        click(&mut context, size, "pages:tab:1", &mut build);
        assert_eq!(probe.borrow().selected, 1);
        context.request_focus("pages");
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowRight,
            }),
            &mut build,
        );
        assert_eq!(probe.borrow().selected, 2);
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowLeft,
            }),
            &mut build,
        );
        assert_eq!(probe.borrow().selected, 1);
        let frame = render_frame(&mut context, size, None, &mut build);
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::TabList && node.label == "pages"));
    }

    #[test]
    fn progress_reports_fraction_and_indeterminate_requests_frames() {
        let size = Size::new(240.0, 120.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let determinate = context.progress("work", 25.0, 100.0);
        let frame = context.produce().unwrap();
        assert_eq!(determinate.fraction, 0.25);
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::ProgressBar && node.value.as_deref() == Some("25%")));
        context.end_frame();

        context.begin_frame(size);
        let indeterminate = context.progress_indeterminate("busy");
        let busy_frame = context.produce().unwrap();
        assert!(indeterminate.indeterminate);
        assert!(busy_frame.needs_animation_frame);
        assert!(busy_frame
            .access_nodes
            .iter()
            .any(|node| node.value.as_deref() == Some("indeterminate")));
    }

    #[test]
    fn progress_clamps_values_and_normalizes_an_invalid_max() {
        let size = Size::new(240.0, 120.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let low = context.progress("low", -5.0, 100.0);
        assert_eq!(low.value, 0.0);
        assert_eq!(low.fraction, 0.0);
        let high = context.progress("high", 150.0, 100.0);
        assert_eq!(high.value, 100.0);
        assert_eq!(high.fraction, 1.0);
        let nan = context.progress("nan", f32::NAN, 100.0);
        assert_eq!(nan.value, 0.0);
        assert_eq!(nan.fraction, 0.0);
        let zero_max = context.progress("zero-max", 10.0, 0.0);
        assert_eq!(zero_max.value, 1.0);
        assert_eq!(zero_max.fraction, 1.0);
        let infinite_max = context.progress("infinite-max", 5.0, f32::INFINITY);
        assert_eq!(infinite_max.value, 1.0);
        assert_eq!(infinite_max.fraction, 1.0);
        let frame = context.produce().unwrap();
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::ProgressBar && node.value.as_deref() == Some("100%")));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::ProgressBar && node.value.as_deref() == Some("0%")));
    }

    #[test]
    fn spinner_draws_spokes_and_requests_the_next_frame() {
        let size = Size::new(120.0, 80.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let state = context.spinner("load");
        let frame = context.produce().unwrap();
        let spokes = frame
            .display
            .commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::Line { .. }))
            .count();
        // MyGo's coreSpinner strokes twelve spokes a twelfth of a turn apart.
        assert_eq!(spokes, 12);
        assert!(state.spinning);
        assert!(frame.needs_animation_frame);
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Spinner));
    }

    #[test]
    fn badge_uses_theme_accent_and_emits_semantics() {
        let size = Size::new(160.0, 80.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let badge = context.badge("42");
        let frame = context.produce().unwrap();
        assert!(badge.width > 0.0 && badge.height > 0.0);
        // MyGo fills the pill with the text color at 10% (`ui/sidebar.go`
        // coreBadge's `t.Text.Alpha(0.1)`), not the accent.
        let theme = context.theme();
        let tint = Color::rgba(
            theme.foreground.r,
            theme.foreground.g,
            theme.foreground.b,
            26,
        );
        assert!(frame.display.commands.iter().any(|command| {
            matches!(command, DrawCommand::RoundedRect { color, .. } if *color == tint)
        }));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Badge && node.label == "42"));
    }

    #[test]
    fn grid_builder_places_equal_cells_with_row_and_column_gaps() {
        let size = Size::new(340.0, 240.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        context
            .id("grid")
            .grid()
            .columns(3)
            .gap(10.0)
            .padding(10.0)
            .size(320.0, 100.0)
            .children(|context| {
                for index in 0..6 {
                    let height = if index % 2 == 0 { 20.0 } else { 40.0 };
                    context.container().size(50.0, height).children(|context| {
                        context.text(&format!("cell{index}"));
                    });
                }
            });
        let frame = context.produce().unwrap();
        let cells = frame
            .access_nodes
            .iter()
            .filter(|node| node.role == Role::Group && node.label == "container")
            .collect::<Vec<_>>();
        assert_eq!(cells.len(), 6);
        let column_width = (320.0 - 20.0 - 20.0) / 3.0;
        for (index, cell) in cells.iter().enumerate() {
            assert!(
                (cell.bounds.width - column_width).abs() < 0.01,
                "cell {index} width {} != {column_width}",
                cell.bounds.width
            );
        }
        let first_row_gap = cells[1].bounds.x - (cells[0].bounds.x + cells[0].bounds.width);
        assert!((first_row_gap - 10.0).abs() < 0.01);
        let row_gap = cells[3].bounds.y - (cells[0].bounds.y + 40.0);
        assert!((row_gap - 10.0).abs() < 0.01, "row gap was {row_gap}");
        assert!(
            (cells[3].bounds.y - (cells[0].bounds.y + 50.0)).abs() < 0.01,
            "rows must use the tallest cell height"
        );
    }

    #[test]
    fn collapsible_click_and_enter_show_and_hide_content() {
        let size = Size::new(280.0, 200.0);
        let mut context = Context::new(size);
        let probe = Rc::new(RefCell::new(CollapsibleProbe::default()));
        let view_probe = Rc::clone(&probe);
        let mut build = move |context: &mut Context| {
            let mut probe = view_probe.borrow_mut();
            context.collapsible("section", &mut probe.open, "Details", |context| {
                context.text("Body copy");
            });
        };
        click(&mut context, size, "section:header", &mut build);
        assert!(
            probe.borrow().open,
            "clicking the header expands the section"
        );
        let frame = render_frame(&mut context, size, None, &mut build);
        assert!(frame.display.commands.iter().any(|command| {
            matches!(command, DrawCommand::Text { text, .. } if text == "Body copy")
        }));

        context.request_focus("section:header");
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert!(
            !probe.borrow().open,
            "Enter on the focused header collapses it"
        );
        let collapsed = render_frame(&mut context, size, None, &mut build);
        assert!(!collapsed.display.commands.iter().any(|command| {
            matches!(command, DrawCommand::Text { text, .. } if text == "Body copy")
        }));
    }

    #[test]
    fn transformed_button_hits_with_inverse_pointer() {
        let size = Size::new(300.0, 240.0);
        let mut context = Context::new(size);
        let scaled_clicks = Rc::new(RefCell::new(0_u32));
        let plain_clicks = Rc::new(RefCell::new(0_u32));
        let view_scaled = Rc::clone(&scaled_clicks);
        let view_plain = Rc::clone(&plain_clicks);
        let transform = Transform {
            translation: Point::new(10.0, 10.0),
            scale: Point::new(2.0, 2.0),
            rotation: 0.0,
        };
        let mut build = move |context: &mut Context| {
            column(context).size(300.0, 80.0).children(|context| {
                context.transform(transform, |context| {
                    if context.id("scaled").button("Scaled").clicked {
                        *view_scaled.borrow_mut() += 1;
                    }
                });
            });
            if context.id("plain").button("Plain").clicked {
                *view_plain.borrow_mut() += 1;
            }
        };

        let initial = render_frame(&mut context, size, None, &mut build);
        let scaled_region = initial
            .interactions
            .iter()
            .find(|region| region.button_id == "scaled")
            .expect("scaled region");
        let scaled_point = Point::new(
            scaled_region.bounds.x + scaled_region.bounds.width * 2.0 / 2.0 + 10.0,
            scaled_region.bounds.y + scaled_region.bounds.height * 2.0 / 2.0 + 10.0,
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: scaled_point.x,
                y: scaled_point.y,
                button: crate::event::PointerButton::Left,
            }),
            &mut build,
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: scaled_point.x,
                y: scaled_point.y,
                button: crate::event::PointerButton::Left,
            }),
            &mut build,
        );
        assert_eq!(*scaled_clicks.borrow(), 1);
        assert_eq!(*plain_clicks.borrow(), 0);

        let plain_point = center_of(&initial, "plain");
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: plain_point.x,
                y: plain_point.y,
                button: crate::event::PointerButton::Left,
            }),
            &mut build,
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: plain_point.x,
                y: plain_point.y,
                button: crate::event::PointerButton::Left,
            }),
            &mut build,
        );
        assert_eq!(*plain_clicks.borrow(), 1);
        assert_eq!(*scaled_clicks.borrow(), 1);
    }

    #[test]
    fn transformed_slider_drag_uses_local_pointer() {
        let size = Size::new(520.0, 120.0);
        let mut context = Context::new(size);
        let value = Rc::new(RefCell::new(0.0_f32));
        let view_value = Rc::clone(&value);
        let transform = Transform {
            translation: Point::new(0.0, 0.0),
            scale: Point::new(1.5, 1.5),
            rotation: 0.0,
        };
        let mut build = move |context: &mut Context| {
            container(context).size(300.0, 50.0).children(|context| {
                context.transform(transform, |context| {
                    context.slider("slider", &mut view_value.borrow_mut(), 0.0, 100.0);
                });
            });
        };
        let initial = render_frame(&mut context, size, None, &mut build);
        let region = initial
            .interactions
            .iter()
            .find(|region| region.button_id == "slider")
            .expect("slider region");
        let local_y = region.bounds.y + region.bounds.height / 2.0;
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: region.bounds.x + 75.0 * 1.5,
                y: local_y,
                button: crate::event::PointerButton::Left,
            }),
            &mut build,
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: region.bounds.x + 150.0 * 1.5,
                y: local_y,
            }),
            &mut build,
        );
        assert!(
            (*value.borrow() - 50.0).abs() < 1.0,
            "transformed slider value was {}",
            value.borrow()
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: region.bounds.x + 150.0 * 1.5,
                y: local_y,
                button: crate::event::PointerButton::Left,
            }),
            &mut build,
        );
    }
}


/// Turns parsed inline spans into the runs MyGo's markdown view styles:
/// strong 600, italic slanted, code monospaced in a tinted pill, links in
/// the accent, and strikethrough struck.
fn markdown_runs(
    spans: &[InlineSpan],
    size: f32,
    weight: u16,
    color: Color,
    theme: &Theme,
) -> Vec<RichTextSpan> {
    spans
        .iter()
        .map(|span| {
            let mut run = RichTextSpan::new(span.text.clone());
            run.font_size = size;
            run.weight = weight;
            run.color = Some(color);
            match &span.style {
                InlineStyle::Plain => {}
                InlineStyle::Bold => run.weight = 600,
                InlineStyle::Italic => run.italic = true,
                InlineStyle::Strikethrough => run.strikethrough = true,
                InlineStyle::Code => {
                    run.mono = true;
                    run.font_size = size * 0.875;
                    run.background = Some(Color::rgba(24, 24, 27, 18));
                    run.background_radius = 3.06;
                }
                InlineStyle::Link { .. } => run.color = Some(theme.accent),
            }
            run
        })
        .collect()
}


/// The kind of one run of Go source, for the editor theme's colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeColor {
    Text,
    Keyword,
    Type,
    Constant,
    Function,
    Property,
    Operator,
    Number,
    String,
    Comment,
}

/// Splits one line of Go into runs with their colors, as the editor's
/// tokenizer names them.
/// The width of one breadcrumb item: its label at the weight it draws in,
/// padded `pad` either side, and at least Space(6) = 24 unless it is the
/// path's last item.
fn crumb_width(label: &str, font_size: f32, last: bool, pad: f32) -> f32 {
    let weight = if last { 500 } else { 400 };
    let text = host_text_width_weighted(label, font_size, false, false, weight)
        .unwrap_or_else(|| measure_text_width(label, font_size));
    let padded = text + pad * 2.0;
    if last {
        padded
    } else {
        padded.max(24.0)
    }
}

/// Columns of a line's indentation, tabs going to tab stops. A blank line
/// takes the larger of the nearest non-blank lines around it, as MyGo's
/// `lineIndent` does.
fn line_indent(lines: &[&str], index: usize, tab_size: usize) -> usize {
    let indent = |line: &str| -> usize {
        let mut column = 0;
        for character in line.chars() {
            match character {
                '\t' => column += tab_size - column % tab_size,
                ' ' => column += 1,
                _ => return column,
            }
        }
        column
    };
    if lines.get(index).is_some_and(|line| !line.trim().is_empty()) {
        return indent(lines[index]);
    }
    let before = (0..index)
        .rev()
        .take(50)
        .find_map(|i| lines.get(i).filter(|line| !line.trim().is_empty()).map(|l| indent(l)))
        .unwrap_or(0);
    let after = (index + 1..lines.len())
        .take(50)
        .find_map(|i| lines.get(i).filter(|line| !line.trim().is_empty()).map(|l| indent(l)))
        .unwrap_or(0);
    before.max(after)
}

/// The display column a character of a line starts at, tabs expanded.
fn display_column(line: &str, characters: usize, tab_size: usize) -> usize {
    let mut column = 0;
    for (seen, character) in line.chars().enumerate() {
        if seen == characters {
            break;
        }
        if character == '\t' {
            column += tab_size - column % tab_size;
        } else {
            column += 1;
        }
    }
    column
}

/// The wavy line MyGo rules under a diagnostic (`squiggle` in
/// `ui/editor/paint.go`): a 4-pixel wave 1.25 either side of `y`.
fn squiggle_path(x: f32, y: f32, width: f32) -> String {
    if width <= 0.0 {
        return String::new();
    }
    const WAVE: f32 = 4.0;
    let mut path = format!("M{x} {y}");
    let mut step = 0;
    let mut i = 0.0_f32;
    while i < width {
        let dy = if step % 2 == 0 { 1.25 } else { -1.25 };
        let next = (x + i + WAVE / 2.0).min(x + width);
        path.push_str(&format!("L{next} {}", y + dy));
        i += WAVE / 2.0;
        step += 1;
    }
    path
}

fn go_token_kinds(line: &str) -> Vec<(String, CodeColor)> {
    go_tokens(line)
}

/// Splits one line of Go into colored runs: keywords and operators red,
/// calls purple, selectors and numbers blue, strings navy, comments gray,
/// as MyGo's editor theme paints them.
fn go_tokens(line: &str) -> Vec<(String, CodeColor)> {
    const KEYWORDS: [&str; 25] = [
        "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough",
        "for", "func", "go", "goto", "if", "import", "interface", "map", "package", "range",
        "return", "select", "struct", "switch", "type", "var",
    ];
    // MyGo's Go language names these built-in types and functions
    // (`ui/editor/languages.go`), which its theme colors apart from text.
    const TYPES: [&str; 22] = [
        "any", "bool", "byte", "comparable", "complex64", "complex128", "error", "float32",
        "float64", "int", "int8", "int16", "int32", "int64", "rune", "string", "uint", "uint8",
        "uint16", "uint32", "uint64", "uintptr",
    ];
    const CONSTANTS: [&str; 4] = ["true", "false", "nil", "iota"];
    const BUILTINS: [&str; 19] = [
        "append", "cap", "clear", "close", "complex", "copy", "delete", "imag", "len", "make",
        "max", "min", "new", "panic", "print", "println", "real", "recover", "complex",
    ];
    let chars: Vec<char> = line.chars().collect();
    let mut runs = Vec::new();
    let mut plain = String::new();
    let push_plain = |runs: &mut Vec<(String, CodeColor)>, plain: &mut String| {
        if !plain.is_empty() {
            runs.push((std::mem::take(plain), CodeColor::Text));
        }
    };
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if character == '/' && chars.get(index + 1) == Some(&'/') {
            push_plain(&mut runs, &mut plain);
            runs.push((chars[index..].iter().collect(), CodeColor::Comment));
            break;
        }
        if character == '"' || character == '\'' || character == '`' {
            push_plain(&mut runs, &mut plain);
            let quote = character;
            let mut end = index + 1;
            while end < chars.len() {
                if chars[end] == '\\' {
                    end += 2;
                    continue;
                }
                if chars[end] == quote {
                    end += 1;
                    break;
                }
                end += 1;
            }
            runs.push((chars[index..end.min(chars.len())].iter().collect(), CodeColor::String));
            index = end;
            continue;
        }
        if character.is_ascii_digit() {
            push_plain(&mut runs, &mut plain);
            let mut end = index + 1;
            while end < chars.len()
                && (chars[end].is_ascii_alphanumeric() || chars[end] == '.' || chars[end] == '_')
            {
                end += 1;
            }
            runs.push((chars[index..end].iter().collect(), CodeColor::Number));
            index = end;
            continue;
        }
        if character.is_alphabetic() || character == '_' {
            push_plain(&mut runs, &mut plain);
            let mut end = index + 1;
            while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            let word: String = chars[index..end].iter().collect();
            let after_dot = index > 0 && chars[index - 1] == '.';
            let mut next = end;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            let calls = chars.get(next) == Some(&'(');
            let kind = if KEYWORDS.contains(&word.as_str()) {
                CodeColor::Keyword
            } else if TYPES.contains(&word.as_str()) {
                CodeColor::Type
            } else if CONSTANTS.contains(&word.as_str()) {
                CodeColor::Constant
            } else if BUILTINS.contains(&word.as_str()) {
                CodeColor::Function
            } else if calls {
                CodeColor::Function
            } else if after_dot {
                CodeColor::Property
            } else {
                CodeColor::Text
            };
            runs.push((word, kind));
            index = end;
            continue;
        }
        if "&|+-*/%=<>!^~".contains(character) {
            push_plain(&mut runs, &mut plain);
            runs.push((character.to_string(), CodeColor::Operator));
            index += 1;
            continue;
        }
        plain.push(character);
        index += 1;
    }
    push_plain(&mut runs, &mut plain);
    runs
}

