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
use super::code_editor::{tokenize_rust_line, CodeEditorResponse, CodeEditorState, CodeTokenKind};
use super::color_picker::{ColorPickerResponse, ColorPickerState, PRESET_COLORS};
use super::combobox::{filter_options, ComboboxResponse, ComboboxState};
use super::controls::{
    BadgeState, CollapsibleState, ProgressState, RadioGroupState, SelectState, SpinnerState,
    SwitchState, TabsState,
};
use super::date_input::{parse_iso_date, DateInputResponse, DateInputState};
use super::dialog::{DialogAction, DialogResponse};
use super::diff::{
    diff_lines, paired_rows, DiffKind, DiffLayout, DiffViewerResponse, DiffViewerState,
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
use super::font::{default_font, RealFontMeasurer};
use super::form::FormFieldState;
use super::grid_view::{GridViewResponse, GridViewState};
use super::icon::{Icon, IconState};
use super::image::{ImageData, ImageState};
use super::inspector::{InspectorItem, InspectorResponse, InspectorTree};
use super::layout::{
    AlignItems, Edges, FlexDirection, JustifyContent, LayoutEngine, LayoutResult, Node, Point,
    Position, Size, Style,
};
use super::link::LinkState;
use super::list::{ListResponse, ListRow, ListState};
use super::markdown::{
    parse_inline, parse_markdown, InlineSpan, InlineStyle, MarkdownBlock, MarkdownResponse,
};
use super::meter::{MeterState, MeterTone};
use super::numeric::{
    filter_numeric_text, parse_decimal_units, DecimalInputOptions, DecimalInputState,
    NumberInputOptions, NumberInputState,
};
use super::overlay::{OverlayResponse, OverlayState};
use super::paint::{Color, DisplayList, DrawCommand, Radius, Rect, TextAlignment, Transform};
use super::performance::{FrameStats, FrameStatsResponse};
use super::range_slider::{RangeKnob, RangeSliderResponse, RangeSliderState};
use super::rating::RatingState;
use super::render::{CommandCounts, RenderStats};
use super::rich_text::RichTextResponse;
use super::router::{BackForwardState, Page, RouterResponse, RouterState};
use super::search_field::SearchFieldState;
use super::segmented::SegmentedResponse;
use super::sidebar::{SidebarResponse, SidebarSection};
use super::split::{
    SplitOrientation, SplitViewResponse, SplitViewState, MAX_SPLIT_RATIO, MIN_SPLIT_RATIO,
};
use super::stepper::StepperState;
use super::table::{SortDirection, TableColumn, TableResponse, TableRow, TableSort, TableState};
use super::text::{layout_text, ApproximateMeasurer};
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
        }
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
    pub fn row(&mut self) -> RowBuilder<'_> {
        let style = Style {
            flex_direction: FlexDirection::Row,
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
        let theme = self.theme.clone();
        let spacing = theme.spacing;
        let font_size = theme.font_sizes.body;
        let icon_size = 14.0;
        let id = self.new_node(Style {
            padding: Edges::symmetric(spacing.sm, spacing.md),
            ..Style::default()
        });
        let text_size = estimated_text_size(label, font_size, None);
        let content_size = Size::new(text_size.width + icon_size + 4.0, text_size.height);
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
        let horizontal = spacing.md;
        let vertical = spacing.sm;
        let hover_bg = mix_color(theme.surface, theme.accent, 0.65);
        let pressed_bg = theme.accent;
        let background = mix_color(
            mix_color(theme.surface, hover_bg, hover_progress.clamp(0.0, 1.0)),
            pressed_bg,
            pressed_progress.clamp(0.0, 1.0),
        );
        let emphasis = hover_progress.max(pressed_progress);
        let text_color = mix_color(theme.foreground, theme.on_accent, emphasis);
        let total_w = content_size.width + horizontal * 2.0;
        let total_h = content_size.height + vertical * 2.0;
        let mut commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width: total_w,
            height: total_h,
            radius: Radius::all(6.0),
            color: background,
        }];
        // Icon
        if let Some(icon) = Icon::builtin(icon_name) {
            commands.push(DrawCommand::TransformPush {
                transform: Transform {
                    translation: Point::new(horizontal, vertical),
                    scale: Point::new(1.0, 1.0),
                    rotation: 0.0,
                },
            });
            commands.extend(icon.to_commands_with_direction(icon_size, text_color, self.direction));
            commands.push(DrawCommand::TransformPop);
        }
        // Label after icon
        commands.push(DrawCommand::Text {
            x: horizontal + icon_size + 4.0,
            y: vertical,
            text: label.to_string(),
            font_size,
            weight: 500,
            color: text_color,
            max_width: None,
            alignment: TextAlignment::Start,
        });
        if focused {
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

    fn button_styled(&mut self, label: &str, primary: bool) -> ButtonState {
        let theme = self.theme.clone();
        let spacing = theme.spacing;
        let font_size = theme.font_sizes.body;
        let id = self.new_node(Style {
            padding: super::layout::Edges::symmetric(spacing.sm, spacing.md),
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
        self.text_field_impl(id, value, None)
    }

    fn text_field_impl(
        &mut self,
        id: &str,
        value: &mut String,
        filter: Option<&dyn Fn(&str) -> String>,
    ) -> TextFieldState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let vertical = theme.spacing.sm;
        let horizontal = theme.spacing.sm;
        self.widget_state_text_field(id);
        let width = self.current_content_width();
        let style = Style {
            width: Some(width),
            height: Some(measured_text_height(font_size) + vertical * 2.0),
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
        let cursor_x = horizontal + measure_text_width(&value[..cursor], font_size);
        let cursor_y = vertical;
        self.widget_states.insert(
            id.to_string(),
            WidgetState::TextField {
                cursor,
                anchor,
                cursor_x,
                cursor_y,
            },
        );
        let height = measured_text_height(font_size) + vertical * 2.0;
        let border = if focused {
            theme.accent
        } else if hovered {
            mix_color(theme.border, theme.accent, 0.35)
        } else {
            theme.border
        };
        let mut commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
            radius: Radius::all(6.0),
            color: theme.surface,
        }];
        if focused {
            if let Some((start, end)) = selection {
                let selection_x = horizontal + measure_text_width(&value[..start], font_size);
                let selection_width = measure_text_width(&value[start..end], font_size);
                commands.push(DrawCommand::Rect {
                    x: selection_x,
                    y: vertical - 1.0,
                    width: selection_width.max(1.0),
                    height: measured_text_height(font_size) + 2.0,
                    color: mix_color(theme.accent, theme.surface, 0.35),
                });
            }
        }
        commands.push(DrawCommand::Text {
            x: horizontal,
            y: vertical,
            text: value.clone(),
            font_size,
            weight: 400,
            color: theme.foreground,
            max_width: Some((width - horizontal * 2.0).max(1.0)),
            alignment: TextAlignment::Start,
        });
        commands.extend(focus_ring(0.0, 0.0, width, height, border));
        if focused {
            let prefix = &value[..cursor];
            let cursor_x = horizontal + measure_text_width(prefix, font_size);
            commands.push(DrawCommand::Line {
                x1: cursor_x,
                y1: vertical,
                x2: cursor_x,
                y2: height - vertical,
                width: 1.0,
                color: theme.accent,
            });
        }
        self.register_access_node(
            node_id,
            Role::TextField,
            "text field",
            Some(value.as_str()),
            focused,
        );
        self.nodes
            .get_mut(&node_id)
            .expect("text field node exists")
            .commands = commands;
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
        let width = self.current_content_width();
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
            let field = self.text_field_impl(&input_id, &mut state.buffer, None);
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
            let height = measured_text_height(theme.font_sizes.body) + theme.spacing.xs * 2.0;
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
            let mut commands = Vec::new();
            if hovered || focused {
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                    radius: Radius::all(3.0),
                    color: mix_color(theme.surface, theme.foreground, 0.06),
                });
            }
            commands.push(DrawCommand::Text {
                x: theme.spacing.xs,
                y: theme.spacing.xs,
                text: text.clone(),
                font_size: theme.font_sizes.body,
                weight: 400,
                color: theme.foreground,
                max_width: Some((width - theme.spacing.xs * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
            if focused {
                commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
            }
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
        let width = self.current_content_width();
        let input_id = format!("{id}:input");
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.xs,
                ..Style::default()
            },
        );
        let mut removed = false;
        for index in 0..tokens.len() {
            let key = format!("{id}:token:{index}");
            if self.clicked_in_previous(&key) {
                tokens.remove(index);
                removed = true;
                self.focused_id = Some(input_id.clone());
                break;
            }
        }
        self.push_parent(root);
        if !tokens.is_empty() {
            self.row().gap(theme.spacing.xs).children(|context| {
                for (index, token) in tokens.iter().enumerate() {
                    let key = format!("{id}:token:{index}");
                    let chip_width = measure_text_width(token, theme.font_sizes.small)
                        + theme.spacing.sm * 2.0
                        + 14.0;
                    let node = context.new_named_node(
                        &key,
                        Style {
                            width: Some(chip_width.max(36.0)),
                            height: Some(22.0),
                            ..Style::default()
                        },
                    );
                    context.register_interaction(node, key.clone());
                    let hovered = context.pointer_in_previous(&key);
                    let commands = vec![
                        DrawCommand::RoundedRect {
                            x: 0.0,
                            y: 0.0,
                            width: chip_width.max(36.0),
                            height: 22.0,
                            radius: Radius::all(11.0),
                            color: mix_color(theme.surface, theme.accent, 0.2),
                        },
                        DrawCommand::Text {
                            x: theme.spacing.sm,
                            y: 3.0,
                            text: token.clone(),
                            font_size: theme.font_sizes.small,
                            weight: 400,
                            color: theme.foreground,
                            max_width: Some((chip_width - theme.spacing.sm * 2.0).max(1.0)),
                            alignment: TextAlignment::Start,
                        },
                        DrawCommand::Text {
                            x: (chip_width.max(36.0)
                                - theme.spacing.xs
                                - measure_text_width("×", theme.font_sizes.small))
                            .max(theme.spacing.sm),
                            y: 3.0,
                            text: "×".to_string(),
                            font_size: theme.font_sizes.small,
                            weight: 400,
                            color: if hovered {
                                theme.foreground
                            } else {
                                theme.muted
                            },
                            max_width: None,
                            alignment: TextAlignment::Start,
                        },
                    ];
                    context.register_access_node(
                        node,
                        Role::Button,
                        &format!("remove token {}", token),
                        None,
                        false,
                    );
                    context
                        .nodes
                        .get_mut(&node)
                        .expect("token chip exists")
                        .commands = commands;
                }
            });
        }
        let field = self.text_field_impl(&input_id, input, None);
        self.pop_parent();
        let mut added = false;
        if field.focused {
            while let Some(position) = input.find(',') {
                let token = input[..position].trim().to_string();
                if !token.is_empty() {
                    tokens.push(token);
                    added = true;
                }
                input.drain(..=position);
            }
            if self.key_pressed(Key::Enter) {
                let token = input.trim().to_string();
                if !token.is_empty() {
                    tokens.push(token);
                    input.clear();
                    added = true;
                }
            }
            if input.is_empty() && self.key_pressed(Key::Backspace) && !tokens.is_empty() {
                tokens.pop();
                removed = true;
            }
            if added || removed {
                self.focused_id = Some(input_id.clone());
            }
        }
        self.register_access_node(root, Role::Group, id, None, false);
        TokenFieldState {
            changed: field.changed || added || removed,
            added,
            removed,
            focused: field.focused,
            hovered: field.hovered,
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
        let field = self.text_field_impl(&input_id, text, None);
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
        let field = self.text_field_impl(&input_id, query, None);
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
            scaled_rgba_commands(data, &mut commands, width as f32, height as f32, false)
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
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.sm,
                ..Style::default()
            },
        );
        self.push_parent(root);
        for block in &blocks {
            match block {
                MarkdownBlock::Heading { level, spans } => {
                    let font_size = match level {
                        1 => 26.0,
                        2 => 22.0,
                        3 => 19.0,
                        4 => 17.0,
                        5 => 15.0,
                        _ => theme.font_sizes.body,
                    };
                    self.inline_row(spans, font_size, true);
                }
                MarkdownBlock::Paragraph(spans) => {
                    self.inline_row(spans, theme.font_sizes.body, false);
                }
                MarkdownBlock::UnorderedList(items) => {
                    for item in items {
                        self.row().gap(theme.spacing.xs).children(|context| {
                            context
                                .text("•")
                                .font_size(theme.font_sizes.body)
                                .color(theme.muted);
                            for span in item {
                                context.inline_span_node(span, theme.font_sizes.body, false);
                            }
                        });
                    }
                }
                MarkdownBlock::OrderedList(items) => {
                    for (index, item) in items.iter().enumerate() {
                        self.row().gap(theme.spacing.xs).children(|context| {
                            context
                                .text(&format!("{}.", index + 1))
                                .font_size(theme.font_sizes.body)
                                .color(theme.muted);
                            for span in item {
                                context.inline_span_node(span, theme.font_sizes.body, false);
                            }
                        });
                    }
                }
                MarkdownBlock::Code { language, lines } => {
                    let font_size = theme.font_sizes.body;
                    let line_height = measured_text_height(font_size);
                    let code_top = if language.is_some() { 26.0 } else { 12.0 };
                    let height = code_top + line_height * lines.len() as f32 + 10.0;
                    let node = self.new_node(Style {
                        width: Some(width),
                        height: Some(height),
                        ..Style::default()
                    });
                    let mut commands = vec![DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height,
                        radius: Radius::all(6.0),
                        color: mix_color(theme.surface, theme.background, 0.65),
                    }];
                    if let Some(language) = language {
                        commands.push(DrawCommand::Text {
                            x: 12.0,
                            y: 8.0,
                            text: language.clone(),
                            font_size: 11.0,
                            weight: 400,
                            color: theme.muted,
                            max_width: Some((width - 24.0).max(1.0)),
                            alignment: TextAlignment::Start,
                        });
                    }
                    commands.push(DrawCommand::Text {
                        x: 12.0,
                        y: code_top,
                        text: lines.join("\n"),
                        font_size,
                        weight: 400,
                        color: theme.foreground,
                        max_width: Some((width - 24.0).max(1.0)),
                        alignment: TextAlignment::Start,
                    });
                    self.nodes
                        .get_mut(&node)
                        .expect("markdown code node exists")
                        .commands = commands;
                }
                MarkdownBlock::Table { headers, rows } => {
                    let font_size = theme.font_sizes.body;
                    let row_height = measured_text_height(font_size) + 10.0;
                    let columns = headers
                        .len()
                        .max(rows.iter().map(Vec::len).max().unwrap_or(0))
                        .max(1);
                    let mut widths = vec![0.0_f32; columns];
                    for (column, header) in headers.iter().enumerate() {
                        widths[column] = widths[column].max(measure_text_width(header, font_size));
                    }
                    for row in rows {
                        for (column, cell) in row.iter().enumerate().take(columns) {
                            widths[column] = widths[column].max(measure_text_width(cell, font_size));
                        }
                    }
                    for width in &mut widths {
                        *width = (*width + 28.0).max(80.0);
                    }
                    let total: f32 = widths.iter().sum();
                    let scale = if total > width { width / total } else { 1.0 };
                    for width in &mut widths {
                        *width *= scale;
                    }
                    let height = row_height * (rows.len() + 1) as f32;
                    let node = self.new_node(Style {
                        width: Some(width),
                        height: Some(height),
                        ..Style::default()
                    });
                    let mut commands = vec![DrawCommand::Rect {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height: row_height,
                        color: mix_color(theme.surface, theme.border, 0.3),
                    }];
                    let mut x = 0.0;
                    for (column, header) in headers.iter().enumerate() {
                        commands.push(DrawCommand::Text {
                            x: x + 12.0,
                            y: (row_height - measured_text_height(font_size)) / 2.0,
                            text: header.clone(),
                            font_size,
                            weight: 600,
                            color: theme.foreground,
                            max_width: Some((widths[column] - 24.0).max(1.0)),
                            alignment: TextAlignment::Start,
                        });
                        x += widths[column];
                    }
                    for (row_index, row) in rows.iter().enumerate() {
                        let y = row_height * (row_index + 1) as f32;
                        let mut x = 0.0;
                        for (column, cell) in row.iter().enumerate().take(columns) {
                            commands.push(DrawCommand::Text {
                                x: x + 12.0,
                                y: y + (row_height - measured_text_height(font_size)) / 2.0,
                                text: cell.clone(),
                                font_size,
                                weight: 400,
                                color: theme.muted,
                                max_width: Some((widths[column] - 24.0).max(1.0)),
                                alignment: TextAlignment::Start,
                            });
                            x += widths[column];
                        }
                    }
                    for row_index in 0..=rows.len() {
                        let y = row_height * row_index as f32;
                        commands.push(DrawCommand::Line {
                            x1: 0.0,
                            y1: y,
                            x2: width,
                            y2: y,
                            width: 1.0,
                            color: theme.border,
                        });
                    }
                    self.nodes
                        .get_mut(&node)
                        .expect("markdown table node exists")
                        .commands = commands;
                }
            }
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

    fn inline_row(&mut self, spans: &[InlineSpan], font_size: f32, heading: bool) {
        self.row().gap(0.0).children(|context| {
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
            color,
            max_width: Some(width.max(1.0)),
            alignment: TextAlignment::Start,
        });
        if matches!(span.style, InlineStyle::Italic) {
            commands.push(DrawCommand::TransformPop);
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
        let width = self.current_content_width();
        let field_height = measured_text_height(theme.font_sizes.body) + theme.spacing.sm * 2.0;
        let gap = theme.spacing.xs;
        let button_width = 26.0;
        let count_width = 42.0;
        let input_id = format!("{id}:input");
        let previous_key = format!("{id}:prev");
        let next_key = format!("{id}:next");
        if matches == 0 {
            *current = 0;
        } else {
            *current = (*current).min(matches - 1);
        }
        let previous_clicked = matches > 0 && self.clicked_in_previous(&previous_key);
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
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(field_height),
                flex_direction: FlexDirection::Row,
                gap,
                align_items: AlignItems::Center,
                ..Style::default()
            },
        );
        let input_width = (width - count_width - button_width * 2.0 - gap * 3.0).max(1.0);
        self.push_parent(root);
        let mut field_changed = false;
        let mut field_focused = false;
        let mut field_hovered = false;
        self.container().width(input_width).children(|context| {
            let field = context.text_field(&input_id, query);
            field_changed = field.changed;
            field_focused = field.focused;
            field_hovered = field.hovered;
        });
        let count = if matches == 0 {
            "0/0".to_string()
        } else {
            format!("{}/{}", *current + 1, matches)
        };
        self.text(&count)
            .font_size(theme.font_sizes.small)
            .color(theme.muted)
            .max_width(count_width);
        for (key, label, enabled) in [
            (previous_key.as_str(), "<", matches > 0),
            (next_key.as_str(), ">", matches > 0),
        ] {
            let node = self.new_named_node(
                key,
                Style {
                    width: Some(button_width),
                    height: Some(24.0),
                    ..Style::default()
                },
            );
            if enabled {
                self.register_interaction(node, key.to_string());
            }
            let hovered = enabled && self.pointer_in_previous(key);
            let mut commands = vec![
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: button_width,
                    height: 24.0,
                    radius: Radius::all(4.0),
                    color: if hovered {
                        mix_color(theme.surface, theme.accent, 0.3)
                    } else {
                        theme.surface
                    },
                },
                DrawCommand::Text {
                    x: 0.0,
                    y: 3.0,
                    text: label.to_string(),
                    font_size: theme.font_sizes.small,
                    weight: 600,
                    color: if enabled {
                        theme.foreground
                    } else {
                        theme.muted
                    },
                    max_width: Some(button_width),
                    alignment: TextAlignment::Center,
                },
            ];
            if !enabled {
                commands[0] = DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: button_width,
                    height: 24.0,
                    radius: Radius::all(4.0),
                    color: mix_color(theme.surface, theme.border, 0.4),
                };
            }
            self.register_access_node(
                node,
                Role::Button,
                if label == "<" {
                    "previous match"
                } else {
                    "next match"
                },
                Some(if enabled { "enabled" } else { "disabled" }),
                false,
            );
            self.nodes
                .get_mut(&node)
                .expect("find bar button exists")
                .commands = commands;
        }
        self.pop_parent();
        if previous_clicked || next_clicked {
            self.focused_id = Some(input_id.clone());
            field_focused = true;
        }
        self.register_access_node(root, Role::Group, id, None, false);
        FindBarState {
            changed: field_changed || previous_clicked || next_clicked,
            current: *current,
            matches,
            query: query.clone(),
            previous_clicked,
            next_clicked,
            focused: field_focused,
            hovered: field_hovered,
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
        let width = self.current_content_width();
        let height = 28.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                gap: theme.spacing.xs,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let back_key = format!("{id}:back");
        let forward_key = format!("{id}:forward");
        let mut back_clicked = false;
        let mut forward_clicked = false;
        for (key, label, enabled) in [
            (back_key.as_str(), "←", state.can_back()),
            (forward_key.as_str(), "→", state.can_forward()),
        ] {
            let button_width = 30.0;
            let node = self.new_named_node(
                key,
                Style {
                    width: Some(button_width),
                    height: Some(24.0),
                    ..Style::default()
                },
            );
            if enabled {
                self.register_interaction(node, key.to_string());
            }
            let hovered = enabled && self.pointer_in_previous(key);
            let mut commands = vec![
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: button_width,
                    height: 24.0,
                    radius: Radius::all(4.0),
                    color: if hovered {
                        mix_color(theme.surface, theme.accent, 0.3)
                    } else {
                        theme.surface
                    },
                },
                DrawCommand::Text {
                    x: 0.0,
                    y: 3.0,
                    text: label.to_string(),
                    font_size: theme.font_sizes.body,
                    weight: 600,
                    color: if enabled {
                        theme.foreground
                    } else {
                        theme.muted
                    },
                    max_width: Some(button_width),
                    alignment: TextAlignment::Center,
                },
            ];
            if !enabled {
                commands[0] = DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: button_width,
                    height: 24.0,
                    radius: Radius::all(4.0),
                    color: mix_color(theme.surface, theme.border, 0.4),
                };
            }
            self.register_access_node(
                node,
                Role::Button,
                if label == "←" { "back" } else { "forward" },
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
        let width = self.current_content_width();
        let field_height = measured_text_height(theme.font_sizes.body) + theme.spacing.sm * 2.0;
        let gap = theme.spacing.xs;
        let clear_width = 24.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(field_height),
                flex_direction: FlexDirection::Row,
                gap,
                align_items: AlignItems::Center,
                ..Style::default()
            },
        );
        let input_id = format!("{id}:input");
        let clear_key = format!("{id}:clear");
        let input_width = (width - clear_width - gap).max(1.0);
        self.push_parent(root);
        let mut input_changed = false;
        let mut input_focused = false;
        let mut input_hovered = false;
        self.container().width(input_width).children(|context| {
            let field = context.text_field(&input_id, text);
            input_changed = field.changed;
            input_focused = field.focused;
            input_hovered = field.hovered;
        });
        let clear = self.new_named_node(
            &clear_key,
            Style {
                width: Some(clear_width),
                height: Some(field_height),
                ..Style::default()
            },
        );
        self.register_interaction(clear, clear_key.clone());
        let clear_hovered = self.pointer_in_previous(&clear_key);
        let clicked = self.clicked_in_previous(&clear_key);
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: clear_width,
                height: field_height,
                radius: Radius::all(4.0),
                color: if clear_hovered {
                    mix_color(theme.surface, theme.foreground, 0.08)
                } else {
                    theme.surface
                },
            },
            DrawCommand::Text {
                x: 0.0,
                y: theme.spacing.sm,
                text: "×".to_string(),
                font_size: theme.font_sizes.body,
                weight: 400,
                color: if clear_hovered {
                    theme.foreground
                } else {
                    theme.muted
                },
                max_width: Some(clear_width),
                alignment: TextAlignment::Center,
            },
        ];
        if clear_hovered {
            commands.push(DrawCommand::Line {
                x1: 8.0,
                y1: field_height / 2.0,
                x2: clear_width - 8.0,
                y2: field_height / 2.0,
                width: 1.0,
                color: theme.border,
            });
        }
        self.register_access_node(clear, Role::Button, "clear search", None, false);
        self.nodes
            .get_mut(&clear)
            .expect("search clear exists")
            .commands = commands;
        self.pop_parent();

        let mut cleared = false;
        if clicked || (input_focused && self.key_pressed(Key::Escape)) {
            text.clear();
            self.focused_id = Some(input_id.clone());
            cleared = true;
        }
        let submitted = input_focused && self.key_pressed(Key::Enter);
        let focused = input_focused || cleared;
        self.register_access_node(root, Role::Group, id, None, false);
        SearchFieldState {
            changed: input_changed || cleared,
            submitted,
            cleared,
            focused,
            hovered: input_hovered,
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
        let anchor = self.previous_regions.get(id).map(|region| region.bounds);
        let mut changed = false;
        let mut preset = None;
        let mut valid_hex = true;
        if state.open {
            if let Some(anchor) = anchor {
                let mut overlay_state = OverlayState::new(true);
                let mut hex = state.hex.clone();
                let mut dismiss = false;
                let response = self.overlay(
                    &format!("{id}:popup"),
                    &mut overlay_state,
                    anchor,
                    |context| {
                        context.column().gap(theme.spacing.xs).children(|context| {
                            let (derived_hue, derived_sat, derived_val) = rgb_to_hsv(*color);
                            if derived_sat > 0 && derived_val > 0 {
                                state.hue = derived_hue;
                            }
                            state.saturation = derived_sat;
                            state.value = derived_val;
                            let field_width = 220.0_f32;
                            let field_height = 120.0_f32;
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
                            field_commands.push(DrawCommand::Circle {
                                x: marker_x,
                                y: marker_y,
                                radius: 6.0,
                                color: Color::rgb(255, 255, 255),
                            });
                            field_commands.push(DrawCommand::Circle {
                                x: marker_x,
                                y: marker_y,
                                radius: 4.0,
                                color: *color,
                            });
                            context
                                .nodes
                                .get_mut(&field_node)
                                .expect("color field exists")
                                .commands = field_commands;

                            let hue_width = 220.0_f32;
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

                            let alpha_width = 220.0_f32;
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

                            let tint_key = format!("{id}:tint");
                            let tint_node = context.new_named_node(
                                &tint_key,
                                Style {
                                    width: Some(220.0),
                                    height: Some(34.0),
                                    ..Style::default()
                                },
                            );
                            context
                                .nodes
                                .get_mut(&tint_node)
                                .expect("tint card exists")
                                .commands = vec![
                                DrawCommand::RoundedRect {
                                    x: 0.0,
                                    y: 0.0,
                                    width: 220.0,
                                    height: 34.0,
                                    radius: Radius::all(8.0),
                                    color: mix_color(theme.surface, *color, 0.18),
                                },
                                DrawCommand::Text {
                                    x: 10.0,
                                    y: 9.0,
                                    text: "Tint on background".to_string(),
                                    font_size: theme.font_sizes.small,
                                    weight: 600,
                                    color: *color,
                                    max_width: Some(200.0),
                                    alignment: TextAlignment::Start,
                                },
                            ];

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

    /// Creates a HH:MM time input with step buttons.
    pub fn time_input(&mut self, id: &str, text: &mut String) -> TimeInputState {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let input_id = format!("{id}:input");
        let input_width = (width - 108.0).max(1.0);
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(30.0),
                flex_direction: FlexDirection::Row,
                gap: theme.spacing.xs,
                align_items: AlignItems::Center,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let mut field_changed = false;
        let mut field_focused = false;
        let mut field_hovered = false;
        self.container().width(input_width).children(|context| {
            let filter = |raw: &str| filter_time_text(raw);
            let field = context.text_field_impl(&input_id, text, Some(&filter));
            field_changed = field.changed;
            field_focused = field.focused;
            field_hovered = field.hovered;
        });
        let (mut hour, mut minute) = parse_time(text).unwrap_or((0, 0));
        let mut changed = field_changed;
        let mut submitted = false;
        if field_focused && self.key_pressed(Key::Enter) && parse_time(text).is_some() {
            let normalized = format_time(hour, minute);
            if normalized != *text {
                *text = normalized;
                changed = true;
            }
            submitted = true;
        }
        let mut step = |context: &mut Context, key: &str, delta_hour: i32, delta_minute: i32| {
            let node = context.new_named_node(
                key,
                Style {
                    width: Some(22.0),
                    height: Some(22.0),
                    ..Style::default()
                },
            );
            context.register_interaction(node, key.to_string());
            let hovered = context.pointer_in_previous(key);
            let clicked = context.clicked_in_previous(key);
            if clicked {
                let before = text.clone();
                if delta_hour != 0 {
                    hour = (hour as i32 + delta_hour).clamp(0, 23) as u32;
                }
                if delta_minute != 0 {
                    minute = (minute as i32 + delta_minute).clamp(0, 59) as u32;
                }
                *text = format_time(hour, minute);
                if *text != before {
                    changed = true;
                }
                context.focused_id = Some(input_id.clone());
            }
            context
                .nodes
                .get_mut(&node)
                .expect("time step exists")
                .commands = vec![
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: 22.0,
                    height: 22.0,
                    radius: Radius::all(4.0),
                    color: if hovered {
                        mix_color(theme.surface, theme.foreground, 0.08)
                    } else {
                        theme.surface
                    },
                },
                DrawCommand::Text {
                    x: 0.0,
                    y: 2.0,
                    text: if delta_hour > 0 {
                        "H+"
                    } else if delta_hour < 0 {
                        "H-"
                    } else if delta_minute > 0 {
                        "M+"
                    } else {
                        "M-"
                    }
                    .to_string(),
                    font_size: 9.0,
                    weight: 600,
                    color: theme.foreground,
                    max_width: Some(22.0),
                    alignment: TextAlignment::Center,
                },
            ];
            context.register_access_node(node, Role::Button, key, None, false);
        };
        step(self, &format!("{id}:hour_dec"), -1, 0);
        step(self, &format!("{id}:hour_inc"), 1, 0);
        step(self, &format!("{id}:minute_dec"), 0, -1);
        step(self, &format!("{id}:minute_inc"), 0, 1);
        self.pop_parent();
        self.register_access_node(root, Role::Group, id, None, false);
        let parsed = parse_time(text);
        let valid = parsed.is_some();
        let (hour, minute) = parsed.unwrap_or((hour, minute));
        TimeInputState {
            changed,
            valid,
            hour,
            minute,
            submitted,
            focused: field_focused,
            hovered: field_hovered,
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
                color: text_color,
                max_width: Some((width - theme.spacing.md * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            },
        ];
        if focused {
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

    /// Creates an equal-width segmented selector.
    pub fn segmented(
        &mut self,
        id: &str,
        selected: &mut usize,
        labels: &[&str],
    ) -> SegmentedResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let height = measured_text_height(theme.font_sizes.body) + theme.spacing.sm * 2.0;
        if labels.is_empty() {
            *selected = 0;
        } else {
            *selected = (*selected).min(labels.len() - 1);
        }
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                gap: theme.spacing.xs,
                padding: Edges::all(2.0),
                ..Style::default()
            },
        );
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
        let inner_width =
            (width - 4.0 - theme.spacing.xs * labels.len().saturating_sub(1) as f32).max(1.0);
        let segment_width = if labels.is_empty() {
            0.0
        } else {
            inner_width / labels.len() as f32
        };
        let mut root_commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
            radius: Radius::all(7.0),
            color: theme.surface,
        }];
        if focused {
            root_commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.nodes
            .get_mut(&root)
            .expect("segmented root exists")
            .commands = root_commands;
        self.push_parent(root);
        for (index, label) in labels.iter().enumerate() {
            let key = format!("{id}:segment:{index}");
            let node = self.new_named_node(
                &key,
                Style {
                    width: Some(segment_width),
                    height: Some((height - 4.0).max(1.0)),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.clone());
            let segment_hovered = self.pointer_in_previous(&key);
            let selected_segment = *selected == index;
            let mut commands = vec![
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: segment_width,
                    height: (height - 4.0).max(1.0),
                    radius: Radius::all(5.0),
                    color: if selected_segment {
                        theme.accent
                    } else if segment_hovered {
                        mix_color(theme.surface, theme.foreground, 0.08)
                    } else {
                        theme.surface
                    },
                },
                DrawCommand::Text {
                    x: 0.0,
                    y: theme.spacing.sm,
                    text: label.to_string(),
                    font_size: theme.font_sizes.body,
                    weight: if selected_segment { 600 } else { 400 },
                    color: if selected_segment {
                        theme.on_accent
                    } else {
                        theme.foreground
                    },
                    max_width: Some(segment_width.max(1.0)),
                    alignment: TextAlignment::Center,
                },
            ];
            if selected_segment {
                commands.push(DrawCommand::Line {
                    x1: 0.0,
                    y1: height - 5.0,
                    x2: segment_width,
                    y2: height - 5.0,
                    width: 1.0,
                    color: theme.accent,
                });
            }
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
        let width = self.current_content_width();
        let lines = diff_lines(old, new);
        let toggle_key = format!("{id}:layout");
        if self.clicked_in_previous(&toggle_key) {
            state.layout = match state.layout {
                DiffLayout::Unified => DiffLayout::SideBySide,
                DiffLayout::SideBySide => DiffLayout::Unified,
            };
        }
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
        let toggle = self.new_named_node(
            &toggle_key,
            Style {
                width: Some(92.0),
                height: Some(22.0),
                ..Style::default()
            },
        );
        self.register_interaction(toggle, toggle_key.clone());
        let hovered = self.pointer_in_previous(&toggle_key);
        self.nodes
            .get_mut(&toggle)
            .expect("diff toggle exists")
            .commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 92.0,
                height: 22.0,
                radius: Radius::all(4.0),
                color: if hovered {
                    mix_color(theme.surface, theme.foreground, 0.08)
                } else {
                    theme.surface
                },
            },
            DrawCommand::Text {
                x: 0.0,
                y: 3.0,
                text: match state.layout {
                    DiffLayout::Unified => "Side by side".to_string(),
                    DiffLayout::SideBySide => "Unified".to_string(),
                },
                font_size: theme.font_sizes.small,
                weight: 500,
                color: theme.foreground,
                max_width: Some(92.0),
                alignment: TextAlignment::Center,
            },
        ];
        self.register_access_node(toggle, Role::Button, "toggle diff layout", None, false);
        let row_height = 18.0;
        let add_background = mix_color(theme.surface, Color::rgb(34, 197, 94), 0.18);
        let remove_background = mix_color(theme.surface, Color::rgb(239, 68, 68), 0.18);
        match state.layout {
            DiffLayout::Unified => {
                for (index, line) in lines.iter().enumerate() {
                    let node = self.new_named_node(
                        &format!("{id}:line:{index}"),
                        Style {
                            width: Some(width),
                            height: Some(row_height),
                            ..Style::default()
                        },
                    );
                    self.register_interaction(node, format!("{id}:line:{index}"));
                    let mut commands = Vec::new();
                    let background = match line.kind {
                        DiffKind::Added => Some(add_background),
                        DiffKind::Removed => Some(remove_background),
                        DiffKind::Same => None,
                    };
                    if let Some(color) = background {
                        commands.push(DrawCommand::RoundedRect {
                            x: 0.0,
                            y: 0.0,
                            width,
                            height: row_height,
                            radius: Radius::all(2.0),
                            color,
                        });
                    }
                    let prefix = match line.kind {
                        DiffKind::Added => "+",
                        DiffKind::Removed => "-",
                        DiffKind::Same => " ",
                    };
                    commands.push(DrawCommand::Text {
                        x: 4.0,
                        y: 1.0,
                        text: format!(
                            "{:>3} {:>3} {}{}",
                            line.old_line.map_or("-".to_string(), |n| n.to_string()),
                            line.new_line.map_or("-".to_string(), |n| n.to_string()),
                            prefix,
                            line.text
                        ),
                        font_size: theme.font_sizes.small,
                        weight: 400,
                        color: theme.foreground,
                        max_width: Some(width),
                        alignment: TextAlignment::Start,
                    });
                    self.nodes
                        .get_mut(&node)
                        .expect("diff line exists")
                        .commands = commands;
                }
            }
            DiffLayout::SideBySide => {
                let half = ((width - theme.spacing.xs) / 2.0).max(1.0);
                for (row, (left, right)) in paired_rows(&lines).into_iter().enumerate() {
                    let node = self.new_named_node(
                        &format!("{id}:line:{row}"),
                        Style {
                            width: Some(width),
                            height: Some(row_height),
                            ..Style::default()
                        },
                    );
                    let mut commands = Vec::new();
                    for (offset, line_index) in [(0.0, left), (half + theme.spacing.xs, right)] {
                        let Some(line_index) = line_index else {
                            continue;
                        };
                        let line = &lines[line_index];
                        let background = match line.kind {
                            DiffKind::Added => Some(add_background),
                            DiffKind::Removed => Some(remove_background),
                            DiffKind::Same => None,
                        };
                        if let Some(color) = background {
                            commands.push(DrawCommand::RoundedRect {
                                x: offset,
                                y: 0.0,
                                width: half,
                                height: row_height,
                                radius: Radius::all(2.0),
                                color,
                            });
                        }
                        commands.push(DrawCommand::Text {
                            x: offset + 4.0,
                            y: 1.0,
                            text: line.text.clone(),
                            font_size: theme.font_sizes.small,
                            weight: 400,
                            color: theme.foreground,
                            max_width: Some(half - 8.0),
                            alignment: TextAlignment::Start,
                        });
                    }
                    self.nodes
                        .get_mut(&node)
                        .expect("diff pair exists")
                        .commands = commands;
                }
            }
        }
        self.pop_parent();
        let added = lines
            .iter()
            .filter(|line| line.kind == DiffKind::Added)
            .count();
        let removed = lines
            .iter()
            .filter(|line| line.kind == DiffKind::Removed)
            .count();
        let same = lines.len() - added - removed;
        self.register_access_node(root, Role::Group, id, None, false);
        DiffViewerResponse {
            lines,
            added,
            removed,
            same,
            layout: state.layout,
        }
    }

    /// Creates a read-only code viewer.
    pub fn code_editor(
        &mut self,
        id: &str,
        code: &str,
        state: &mut CodeEditorState,
    ) -> CodeEditorResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let line_height = 20.0;
        let lines = code.lines().collect::<Vec<_>>();
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
        let mut root_commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
            radius: Radius::all(6.0),
            color: theme.background,
        }];
        if focused {
            root_commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.nodes
            .get_mut(&root)
            .expect("code root exists")
            .commands = root_commands;
        self.push_parent(root);
        let gutter = 36.0;
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
            let mut commands = Vec::new();
            if selected || hovered {
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height: line_height,
                    radius: Radius::all(2.0),
                    color: if selected {
                        mix_color(theme.accent, theme.background, 0.15)
                    } else {
                        mix_color(theme.surface, theme.foreground, 0.05)
                    },
                });
            }
            commands.push(DrawCommand::Text {
                x: 4.0,
                y: 2.0,
                text: (index + 1).to_string(),
                font_size: theme.font_sizes.small,
                weight: 400,
                color: theme.muted,
                max_width: Some(gutter - 8.0),
                alignment: TextAlignment::End,
            });
            let mut x = gutter + 6.0;
            for token in tokenize_rust_line(line) {
                if x >= width {
                    break;
                }
                let token_width = measure_text_width(&token.text, theme.font_sizes.small);
                let color = match token.kind {
                    CodeTokenKind::Keyword => theme.accent,
                    CodeTokenKind::String => Color::rgb(134, 239, 172),
                    CodeTokenKind::Comment => theme.muted,
                    CodeTokenKind::Number => Color::rgb(251, 191, 36),
                    CodeTokenKind::Text => theme.foreground,
                };
                commands.push(DrawCommand::Text {
                    x,
                    y: 2.0,
                    text: token.text,
                    font_size: theme.font_sizes.small,
                    weight: 400,
                    color,
                    max_width: Some((width - x).max(1.0)),
                    alignment: TextAlignment::Start,
                });
                x += token_width;
            }
            self.register_access_node(
                node,
                Role::Button,
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
        let theme = self.theme.clone();
        state.month = state.month.clamp(1, 12);
        let width = self.current_content_width();
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
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Column,
                gap: theme.spacing.xs,
                ..Style::default()
            },
        );
        self.register_interaction(root, id.to_string());
        self.register_focusable(root, id.to_string());
        let mut root_commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height: 250.0,
            radius: Radius::all(8.0),
            color: theme.surface,
        }];
        if focused {
            root_commands.extend(focus_ring(0.0, 0.0, width, 250.0, theme.accent));
        }
        self.nodes
            .get_mut(&root)
            .expect("calendar root exists")
            .commands = root_commands;
        self.push_parent(root);
        let header_height = 28.0;
        self.row()
            .gap(theme.spacing.xs)
            .height(header_height)
            .children(|context| {
                let nav = |context: &mut Context, key: &str, label: &str| {
                    let button = context.new_named_node(
                        key,
                        Style {
                            width: Some(24.0),
                            height: Some(24.0),
                            ..Style::default()
                        },
                    );
                    context.register_interaction(button, key.to_string());
                    context
                        .nodes
                        .get_mut(&button)
                        .expect("calendar nav exists")
                        .commands = vec![
                        DrawCommand::RoundedRect {
                            x: 0.0,
                            y: 0.0,
                            width: 24.0,
                            height: 24.0,
                            radius: Radius::all(4.0),
                            color: theme.surface,
                        },
                        DrawCommand::Text {
                            x: 0.0,
                            y: 3.0,
                            text: label.to_string(),
                            font_size: theme.font_sizes.small,
                            weight: 600,
                            color: theme.foreground,
                            max_width: Some(24.0),
                            alignment: TextAlignment::Center,
                        },
                    ];
                    context.register_access_node(button, Role::Button, label, None, false);
                };
                nav(context, &previous_key, "<");
                context
                    .text(&format!("{:04}-{:02}", state.year, state.month))
                    .font_size(theme.font_sizes.body)
                    .weight(600)
                    .color(theme.foreground)
                    .max_width(width - 56.0);
                nav(context, &next_key, ">");
            });
        let cell_width = ((width - theme.spacing.xs * 6.0) / 7.0).max(1.0);
        let weekday_row = self.new_named_node(
            &format!("{id}:weekdays"),
            Style {
                width: Some(width),
                height: Some(18.0),
                ..Style::default()
            },
        );
        let mut weekday_commands = Vec::new();
        for (index, label) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
            weekday_commands.push(DrawCommand::Text {
                x: index as f32 * (cell_width + theme.spacing.xs),
                y: 1.0,
                text: (*label).to_string(),
                font_size: theme.font_sizes.small,
                weight: 400,
                color: theme.muted,
                max_width: Some(cell_width),
                alignment: TextAlignment::Center,
            });
        }
        self.nodes
            .get_mut(&weekday_row)
            .expect("calendar weekdays exist")
            .commands = weekday_commands;
        let leading = weekday_monday(state.year, state.month, 1) as usize;
        let days = days_in_month(state.year, state.month);
        for row in 0..6 {
            self.row().gap(theme.spacing.xs).children(|context| {
                for column in 0..7 {
                    let position = row * 7 + column;
                    let day = position - leading as i32 + 1;
                    if day < 1 || day > days as i32 {
                        context.container().size(cell_width, 24.0);
                        continue;
                    }
                    let day = day as u32;
                    let key = format!("{id}:day:{day}");
                    let node = context.new_named_node(
                        &key,
                        Style {
                            width: Some(cell_width),
                            height: Some(24.0),
                            ..Style::default()
                        },
                    );
                    context.register_interaction(node, key.clone());
                    let hovered = context.pointer_in_previous(&key);
                    let selected = state.selected == Some(day);
                    let mut commands = Vec::new();
                    if selected || hovered {
                        commands.push(DrawCommand::RoundedRect {
                            x: 0.0,
                            y: 0.0,
                            width: cell_width,
                            height: 24.0,
                            radius: Radius::all(4.0),
                            color: if selected {
                                theme.accent
                            } else {
                                mix_color(theme.surface, theme.foreground, 0.08)
                            },
                        });
                    }
                    commands.push(DrawCommand::Text {
                        x: 0.0,
                        y: 4.0,
                        text: day.to_string(),
                        font_size: theme.font_sizes.small,
                        weight: if selected { 600 } else { 400 },
                        color: if selected {
                            theme.on_accent
                        } else {
                            theme.foreground
                        },
                        max_width: Some(cell_width),
                        alignment: TextAlignment::Center,
                    });
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

    /// Creates an ISO date input with a calendar overlay.
    pub fn date_input(
        &mut self,
        id: &str,
        text: &mut String,
        state: &mut DateInputState,
    ) -> DateInputResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let field_height = measured_text_height(theme.font_sizes.body) + theme.spacing.sm * 2.0;
        let input_id = format!("{id}:input");
        let toggle_key = format!("{id}:toggle");
        let popup_id = format!("{id}:popup");
        let input_width = (width - 30.0 - theme.spacing.xs).max(1.0);
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(field_height),
                flex_direction: FlexDirection::Row,
                gap: theme.spacing.xs,
                align_items: AlignItems::Center,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let mut field_changed = false;
        let mut field_focused = false;
        self.container().width(input_width).children(|context| {
            let field = context.text_field(&input_id, text);
            field_changed = field.changed;
            field_focused = field.focused;
        });
        let toggle = self.new_named_node(
            &toggle_key,
            Style {
                width: Some(30.0),
                height: Some(24.0),
                ..Style::default()
            },
        );
        self.register_interaction(toggle, toggle_key.clone());
        let toggle_hovered = self.pointer_in_previous(&toggle_key);
        let toggle_clicked = self.clicked_in_previous(&toggle_key);
        self.nodes
            .get_mut(&toggle)
            .expect("date toggle exists")
            .commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 30.0,
                height: 24.0,
                radius: Radius::all(4.0),
                color: if toggle_hovered {
                    mix_color(theme.surface, theme.foreground, 0.08)
                } else {
                    theme.surface
                },
            },
            DrawCommand::Text {
                x: 0.0,
                y: 3.0,
                text: "v".to_string(),
                font_size: theme.font_sizes.small,
                weight: 600,
                color: theme.foreground,
                max_width: Some(30.0),
                alignment: TextAlignment::Center,
            },
        ];
        self.register_access_node(toggle, Role::Button, "calendar", None, false);
        self.pop_parent();
        let parsed = parse_iso_date(text);
        let mut changed = field_changed;
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
            if field_focused && self.key_pressed(Key::Enter) {
                submitted = true;
                state.open = false;
                self.focused_id = Some(input_id.clone());
            }
        }
        if toggle_clicked {
            state.open = !state.open;
            if state.open {
                if let Some((year, month, day)) = parsed {
                    state.calendar.year = year;
                    state.calendar.month = month;
                    state.calendar.selected = Some(day);
                }
            } else {
                self.focused_id = Some(input_id.clone());
            }
        }
        let input_bounds = self
            .previous_regions
            .get(&input_id)
            .map(|region| region.bounds);
        let mut calendar_changed = false;
        if state.open {
            if let Some(anchor) = input_bounds {
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
                        self.focused_id = Some(input_id.clone());
                    }
                }
                if !state.open {
                    self.focused_id = Some(input_id.clone());
                }
            }
        }
        let valid = parse_iso_date(text).is_some();
        self.register_access_node(root, Role::Group, id, None, false);
        DateInputResponse {
            open: state.open,
            valid,
            changed: changed || calendar_changed,
            submitted,
            focused: field_focused,
        }
    }

    /// Creates a clickable text link.
    pub fn link(&mut self, id: &str, label: &str) -> LinkState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = measure_text_width(label, font_size) + theme.spacing.xs * 2.0;
        let height = measured_text_height(font_size) + theme.spacing.xs * 2.0;
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
            x: theme.spacing.xs,
            y: theme.spacing.xs,
            text: label.to_string(),
            font_size,
            weight: 500,
            color: if hovered || focused {
                theme.foreground
            } else {
                theme.accent
            },
            max_width: Some(width - theme.spacing.xs * 2.0),
            alignment: TextAlignment::Start,
        }];
        if hovered || focused {
            commands.push(DrawCommand::Line {
                x1: theme.spacing.xs,
                y1: height - theme.spacing.xs,
                x2: width - theme.spacing.xs,
                y2: height - theme.spacing.xs,
                width: 1.0,
                color: theme.accent,
            });
        }
        if focused {
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
        let theme = self.theme.clone();
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
        let mut commands = Vec::new();
        let mut drawn = false;
        if size > 0.0 {
            commands.push(match shape {
                AvatarShape::Circle => DrawCommand::Circle {
                    x: size / 2.0,
                    y: size / 2.0,
                    radius: size / 2.0,
                    color: mix_color(theme.surface, theme.accent, 0.25),
                },
                AvatarShape::Rounded => DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: size,
                    height: size,
                    radius: Radius::all(size * 0.25),
                    color: mix_color(theme.surface, theme.accent, 0.25),
                },
            });
            if let Some(data) = data {
                let (columns, rows) = scaled_rgba_commands(
                    data,
                    &mut commands,
                    size,
                    size,
                    shape == AvatarShape::Circle,
                );
                drawn = columns > 0 && rows > 0;
            }
            if let Some(initials) = initials {
                let font_size = (size * 0.4).clamp(10.0, 32.0);
                let text_width = measure_text_width(initials, font_size);
                commands.push(DrawCommand::Text {
                    x: (size - text_width) / 2.0,
                    y: (size - measured_text_height(font_size)) / 2.0,
                    text: initials.to_string(),
                    font_size,
                    weight: 700,
                    color: theme.foreground,
                    max_width: Some((size * 0.8).max(1.0)),
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
        let tone = if value < low {
            MeterTone::Low
        } else if value < high {
            MeterTone::Medium
        } else {
            MeterTone::High
        };
        let width = self.current_content_width();
        let height = 10.0;
        let node = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        let tone_color = match tone {
            MeterTone::Low => theme.muted,
            MeterTone::Medium => theme.accent,
            MeterTone::High => theme.warn(),
        };
        let mut commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
            radius: Radius::all(5.0),
            color: theme.border,
        }];
        if fraction > 0.0 {
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: (width * fraction).max(2.0),
                height,
                radius: Radius::all(5.0),
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
        let height = 28.0;
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
        let low_x = low_fraction * (width - 16.0) + 8.0;
        let high_x = high_fraction * (width - 16.0) + 8.0;
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: height / 2.0 - 3.0,
                width,
                height: 6.0,
                radius: Radius::all(3.0),
                color: theme.border,
            },
            DrawCommand::RoundedRect {
                x: low_x,
                y: height / 2.0 - 3.0,
                width: (high_x - low_x).max(0.0),
                height: 6.0,
                radius: Radius::all(3.0),
                color: theme.accent,
            },
            DrawCommand::Circle {
                x: low_x,
                y: height / 2.0,
                radius: if state.active == RangeKnob::Low {
                    8.0
                } else {
                    7.0
                },
                color: if state.active == RangeKnob::Low {
                    theme.accent
                } else {
                    mix_color(theme.accent, theme.surface, 0.45)
                },
            },
            DrawCommand::Circle {
                x: high_x,
                y: height / 2.0,
                radius: if state.active == RangeKnob::High {
                    8.0
                } else {
                    7.0
                },
                color: if state.active == RangeKnob::High {
                    theme.accent
                } else {
                    mix_color(theme.accent, theme.surface, 0.45)
                },
            },
        ];
        if focused {
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
        let width = self.current_content_width();
        *value = (*value).min(max);
        let star_size = 26.0;
        let gap = theme.spacing.xs;
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
        let mut root_commands = Vec::new();
        if focused {
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
            let star_color = if selected {
                theme.accent
            } else if hovered {
                theme.foreground
            } else {
                theme.muted
            };
            let mut commands = vec![
                DrawCommand::TransformPush {
                    transform: Transform {
                        translation: Point::new(0.0, 0.0),
                        scale: Point::new(star_size / 24.0, star_size / 24.0),
                        rotation: 0.0,
                    },
                },
                DrawCommand::SvgPath {
                    x: 0.0,
                    y: 0.0,
                    width: 24.0,
                    height: 24.0,
                    path: STAR_PATH.to_string(),
                    fill: selected.then_some(star_color),
                    stroke: (!selected).then_some(star_color),
                    stroke_width: 1.4,
                },
                DrawCommand::TransformPop,
            ];
            if hovered {
                commands.insert(
                    0,
                    DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: star_size,
                        height: star_size,
                        radius: Radius::all(4.0),
                        color: mix_color(theme.surface, theme.accent, 0.16),
                    },
                );
            }
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
        let width = self.current_content_width();
        let height = 30.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                padding: Edges::symmetric(theme.spacing.xs, theme.spacing.xs),
                gap: theme.spacing.xs,
                align_items: AlignItems::Center,
                ..Style::default()
            },
        );
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
        let mut root_commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
            radius: Radius::all(6.0),
            color: theme.surface,
        }];
        if focused {
            root_commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.nodes
            .get_mut(&root)
            .expect("stepper root exists")
            .commands = root_commands;
        self.push_parent(root);
        let button_width = 28.0;
        for (key, label, delta) in [
            (decrease_key.as_str(), "-", -1_i64),
            (increase_key.as_str(), "+", 1_i64),
        ] {
            let node = self.new_named_node(
                key,
                Style {
                    width: Some(button_width),
                    height: Some(22.0),
                    ..Style::default()
                },
            );
            self.register_interaction(node, key.to_string());
            let hovered = self.pointer_in_previous(key);
            let mut commands = vec![
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: button_width,
                    height: 22.0,
                    radius: Radius::all(4.0),
                    color: if hovered {
                        mix_color(theme.surface, theme.accent, 0.35)
                    } else {
                        mix_color(theme.surface, theme.border, 0.6)
                    },
                },
                DrawCommand::Text {
                    x: 0.0,
                    y: 3.0,
                    text: label.to_string(),
                    font_size: theme.font_sizes.body,
                    weight: 600,
                    color: theme.foreground,
                    max_width: Some(button_width),
                    alignment: TextAlignment::Center,
                },
            ];
            if delta < 0 && *value <= min {
                commands[0] = DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: button_width,
                    height: 22.0,
                    radius: Radius::all(4.0),
                    color: theme.surface,
                };
            }
            self.register_access_node(node, Role::Button, label, None, false);
            self.nodes
                .get_mut(&node)
                .expect("stepper button exists")
                .commands = commands;
        }
        let value_width = (width - button_width * 2.0 - theme.spacing.xs * 2.0).max(1.0);
        let text_node = self.new_named_node(
            &format!("{id}:value"),
            Style {
                width: Some(value_width),
                height: Some(22.0),
                ..Style::default()
            },
        );
        self.update_text(
            text_node,
            &value.to_string(),
            theme.font_sizes.body,
            600,
            theme.foreground,
            Some(value_width),
            TextAlignment::Center,
        );
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
        let field = self.text_field_impl(id, text, Some(&filter));
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
        let field = self.text_field_impl(id, text, Some(&filter));
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
        self.checkbox_impl(id, None, checked)
    }

    /// Creates a checkbox that shows a caller-supplied label next to the box.
    pub fn checkbox_labeled(
        &mut self,
        id: &str,
        label: &str,
        checked: &mut bool,
    ) -> CheckboxState {
        self.checkbox_impl(id, Some(label), checked)
    }

    fn checkbox_impl(
        &mut self,
        id: &str,
        label: Option<&str>,
        checked: &mut bool,
    ) -> CheckboxState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        if !matches!(self.widget_states.get(id), Some(WidgetState::Checkbox)) {
            self.widget_states
                .insert(id.to_string(), WidgetState::Checkbox);
        }
        let height = measured_text_height(font_size).max(22.0);
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
            *checked = !*checked;
        }
        let mut commands = Vec::new();
        if *checked {
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: (height - 18.0) / 2.0,
                width: 18.0,
                height: 18.0,
                radius: Radius::all(4.0),
                color: theme.accent,
            });
        } else {
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: (height - 18.0) / 2.0,
                width: 18.0,
                height: 18.0,
                radius: Radius::all(4.0),
                color: theme.border,
            });
            commands.push(DrawCommand::RoundedRect {
                x: 1.0,
                y: (height - 18.0) / 2.0 + 1.0,
                width: 16.0,
                height: 16.0,
                radius: Radius::all(3.0),
                color: theme.surface,
            });
        }
        commands.push(DrawCommand::Text {
            x: 26.0,
            y: (height - measured_text_height(font_size)) / 2.0,
            text: label
                .map(str::to_string)
                .unwrap_or_else(|| if *checked { "checked" } else { "unchecked" }.to_string()),
            font_size,
            weight: 400,
            color: theme.foreground,
            max_width: None,
            alignment: TextAlignment::Start,
        });
        if *checked {
            commands.push(DrawCommand::Line {
                x1: 5.0,
                y1: height / 2.0,
                x2: 8.0,
                y2: height / 2.0 + 3.0,
                width: 2.0,
                color: theme.on_accent,
            });
            commands.push(DrawCommand::Line {
                x1: 8.0,
                y1: height / 2.0 + 3.0,
                x2: 14.0,
                y2: height / 2.0 - 4.0,
                width: 2.0,
                color: theme.on_accent,
            });
        }
        if focused {
            commands.extend(focus_ring(0.0, 0.0, width, height, theme.accent));
        }
        self.register_access_node(
            node_id,
            Role::CheckBox,
            "checkbox",
            Some(if *checked { "checked" } else { "unchecked" }),
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
        let theme = self.theme.clone();
        let min = min.min(max);
        let max = max.max(min);
        *value = value.clamp(min, max);
        if !matches!(self.widget_states.get(id), Some(WidgetState::Slider { .. })) {
            self.widget_states
                .insert(id.to_string(), WidgetState::Slider { dragging: false });
        }
        let height = 28.0;
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
                        let next = min + (max - min) * fraction;
                        if (*value - next).abs() > f32::EPSILON {
                            *value = next;
                            changed = true;
                        }
                    }
                }
                if !pointer_down {
                    *dragging = false;
                }
                let keyboard_step = (max - min) / 100.0;
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
        let knob_x = fraction * (width - 16.0) + 8.0;
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: height / 2.0 - 3.0,
                width,
                height: 6.0,
                radius: Radius::all(3.0),
                color: theme.border,
            },
            DrawCommand::RoundedRect {
                x: 0.0,
                y: height / 2.0 - 3.0,
                width: knob_x,
                height: 6.0,
                radius: Radius::all(3.0),
                color: theme.accent,
            },
            DrawCommand::Circle {
                x: knob_x,
                y: height / 2.0,
                radius: 8.0,
                color: if focused || dragging {
                    mix_color(theme.accent, theme.foreground, 0.35)
                } else {
                    theme.accent
                },
            },
        ];
        if focused {
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
        let theme = self.theme.clone();
        let viewport_height = viewport_height.max(1.0);
        let width = self.current_content_width();
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
                ..Style::default()
            },
        );
        self.register_interaction(node_id, id.to_string());
        self.push_parent(node_id);
        content(self);
        self.pop_parent();
        let content_height = self.estimated_content_height(node_id, width);
        let max_scroll = (content_height - viewport_height).max(0.0);
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
        let node = self.nodes.get_mut(&node_id).expect("scroll node exists");
        node.commands = vec![DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height: viewport_height,
            radius: Radius::all(6.0),
            color: theme.surface,
        }];
        node.overlay_commands = vec![
            DrawCommand::RoundedRect {
                x: width - 7.0,
                y: 0.0,
                width: 5.0,
                height: viewport_height,
                radius: Radius::all(2.5),
                color: theme.border,
            },
            DrawCommand::RoundedRect {
                x: width - 8.0,
                y: thumb_y,
                width: 7.0,
                height: thumb_height,
                radius: Radius::all(3.5),
                color: theme.muted,
            },
        ];
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
                } else if key == Key::ShiftTab {
                    self.cycle_focus(true);
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
            let mut local = DisplayList::from_commands(pending.commands.clone());
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
            }
        }
        for child in &pending.children {
            self.append_display(*child, layout, display);
        }
        if pending.scroll.is_some() {
            display.push(DrawCommand::TransformPop);
            display.push(DrawCommand::ScissorPop);
        }
        if let Some(result) = result {
            let mut overlay = DisplayList::from_commands(pending.overlay_commands.clone());
            overlay.translate(Point::new(result.x, result.y));
            display.extend(overlay.commands);
        }
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

    fn new_node(&mut self, style: Style) -> u64 {
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
    ) {
        let size = estimated_text_size(text, font_size, max_width);
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        node.content_size = size;
        node.commands = vec![DrawCommand::Text {
            x: 0.0,
            y: 0.0,
            text: text.to_string(),
            font_size,
            weight,
            color,
            max_width,
            alignment,
        }];
    }

    fn push_parent(&mut self, id: u64) {
        self.parent_stack.push(id);
    }

    fn pop_parent(&mut self) {
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

    fn current_content_width(&self) -> f32 {
        let parent = self.parent_stack.last().and_then(|id| self.nodes.get(id));
        let width = parent
            .and_then(|node| node.style.width)
            .unwrap_or(self.available_size.width);
        let padding = parent
            .map(|node| node.style.padding.horizontal())
            .unwrap_or(0.0);
        (width - padding).max(1.0)
    }

    fn current_content_height(&self) -> f32 {
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
        self.input.keys.contains(&key)
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
        let width = anchor
            .width
            .max(220.0)
            .min(self.available_size.width.max(220.0));
        let x = anchor
            .x
            .clamp(0.0, (self.available_size.width - width).max(0.0));
        let gap = theme.spacing.xs;
        let mut y = anchor.y + anchor.height + gap;
        let saved_parents = std::mem::take(&mut self.parent_stack);
        self.explicit_id = Some(id.to_string());
        let node_id = self.new_node(Style {
            width: Some(width),
            position: Position::Absolute,
            left: Some(x),
            top: Some(y),
            flex_direction: FlexDirection::Column,
            padding: Edges::all(theme.spacing.sm),
            gap: theme.spacing.xs,
            ..Style::default()
        });
        self.push_parent(node_id);
        content(self);
        self.pop_parent();
        self.parent_stack = saved_parents;
        self.current_layer = previous_layer;

        let max_height = (self.available_size.height - 8.0).max(36.0);
        let height = (self.estimated_content_height(node_id, width) + theme.spacing.sm * 2.0)
            .max(36.0)
            .min(max_height);
        if y + height > self.available_size.height && anchor.y - height - gap >= 0.0 {
            y = anchor.y - height - gap;
        }
        let node = self.nodes.get_mut(&node_id).expect("overlay node exists");
        node.style.top = Some(y);
        node.style.height = Some(height);
        node.scroll = Some(ScrollNode {
            offset: 0.0,
            viewport_height: height,
        });
        node.commands = vec![
            DrawCommand::RoundedRect {
                x: 2.0,
                y: 4.0,
                width,
                height,
                radius: Radius::all(10.0),
                color: Color::rgba(0, 0, 0, 96),
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
        let width = 42.0;
        let height = 24.0;
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
        let track = if *checked {
            theme.accent
        } else {
            mix_color(theme.surface, theme.border, 0.4)
        };
        let knob = if *checked {
            theme.on_accent
        } else {
            mix_color(theme.foreground, theme.muted, 0.3)
        };
        let center_x = 11.0 + progress * 20.0;
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 1.0,
                y: 1.0,
                width: 40.0,
                height: 22.0,
                radius: Radius::all(11.0),
                color: track,
            },
            DrawCommand::Circle {
                x: center_x,
                y: height / 2.0,
                radius: 8.0,
                color: knob,
            },
        ];
        if focused {
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
        if focused {
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
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        let padding_x = theme.spacing.sm;
        let padding_y = theme.spacing.xs;
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
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius: Radius::all(6.0),
                color: theme.surface,
            },
            DrawCommand::Text {
                x: padding_x,
                y: padding_y,
                text: selected_label.to_string(),
                font_size,
                weight: 400,
                color: theme.foreground,
                max_width: Some((width - padding_x * 2.0 - 12.0).max(1.0)),
                alignment: TextAlignment::Start,
            },
        ];
        let arrow_y = height / 2.0;
        commands.push(DrawCommand::Line {
            x1: width - padding_x - 8.0,
            y1: arrow_y - 2.0,
            x2: width - padding_x - 3.0,
            y2: arrow_y + 3.0,
            width: 1.5,
            color: if open { theme.accent } else { theme.muted },
        });
        commands.push(DrawCommand::Line {
            x1: width - padding_x - 3.0,
            y1: arrow_y + 3.0,
            x2: width - padding_x + 2.0,
            y2: arrow_y - 2.0,
            width: 1.5,
            color: if open { theme.accent } else { theme.muted },
        });
        if focused || open {
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
        let height = measured_text_height(font_size) + theme.spacing.sm * 2.0;
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                gap: theme.spacing.xs,
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
            let tab_width = measure_text_width(label, font_size) + theme.spacing.md * 2.0;
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
            let selected_tab = index == *selected;
            let color = if selected_tab {
                theme.accent
            } else if tab_hovered {
                mix_color(theme.muted, theme.foreground, 0.5)
            } else {
                theme.muted
            };
            let mut commands = vec![DrawCommand::Text {
                x: theme.spacing.md,
                y: theme.spacing.sm,
                text: label.to_string(),
                font_size,
                weight: if selected_tab { 600 } else { 400 },
                color,
                max_width: Some((tab_width - theme.spacing.md * 2.0).max(1.0)),
                alignment: TextAlignment::Center,
            }];
            if selected_tab {
                commands.push(DrawCommand::Rect {
                    x: 0.0,
                    y: height - 2.0,
                    width: tab_width,
                    height: 2.0,
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
        if focused {
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
        let height = 8.0;
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
        let height = 8.0;
        let phase = if self.reduce_motion {
            0.0
        } else {
            (self.frame_serial as f32 * 0.02) % 1.0
        };
        if !self.reduce_motion {
            self.request_animation_frame();
        }
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(width),
                height: Some(height),
                ..Style::default()
            },
        );
        let segment = (width * 0.3).max(8.0);
        let x = phase * (width - segment).max(0.0);
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
                radius: Radius::all(4.0),
                color: theme.border,
            },
            DrawCommand::RoundedRect {
                x,
                y: 0.0,
                width: segment,
                height,
                radius: Radius::all(4.0),
                color: theme.accent,
            },
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
        let size = 18.0;
        let node_id = self.new_named_node(
            id,
            Style {
                width: Some(size),
                height: Some(size),
                ..Style::default()
            },
        );
        let spinner_active = !self.reduce_motion;
        let angle = if spinner_active {
            self.request_animation_frame();
            (self.frame_serial as f32) * 0.15
        } else {
            0.0
        };
        let mut commands = Vec::new();
        for index in 0..8 {
            let spoke = angle + index as f32 * std::f32::consts::TAU / 8.0;
            let inner = 2.0;
            let outer = 8.0;
            let alpha = 0.2 + 0.8 * (index as f32 / 7.0);
            commands.push(DrawCommand::Line {
                x1: size / 2.0 + spoke.cos() * inner,
                y1: size / 2.0 + spoke.sin() * inner,
                x2: size / 2.0 + spoke.cos() * outer,
                y2: size / 2.0 + spoke.sin() * outer,
                width: 2.0,
                color: mix_color(theme.muted, theme.accent, alpha),
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
            angle,
            spinning: spinner_active,
        }
    }

    /// Creates a short text pill.
    pub fn badge(&mut self, label: &str) -> BadgeState {
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.small;
        let text_width = measure_text_width(label, font_size);
        let text_height = measured_text_height(font_size);
        let width = text_width + theme.spacing.sm * 2.0;
        let height = (text_height + theme.spacing.xs * 2.0).max(18.0);
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
                radius: Radius::all(height / 2.0),
                color: theme.accent,
            },
            DrawCommand::Text {
                x: theme.spacing.sm,
                y: (height - text_height) / 2.0,
                text: label.to_string(),
                font_size,
                weight: 600,
                color: theme.on_accent,
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
        let theme = self.theme.clone();
        let font_size = theme.font_sizes.body;
        let width = self.current_content_width();
        let header_height = measured_text_height(font_size).max(20.0) + theme.spacing.xs * 2.0;
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
                width: Some(width),
                height: Some(header_height),
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
        let mut commands = vec![DrawCommand::Text {
            x: 18.0,
            y: theme.spacing.xs,
            text: title.to_string(),
            font_size,
            weight: 600,
            color: theme.foreground,
            max_width: Some((width - 18.0).max(1.0)),
            alignment: TextAlignment::Start,
        }];
        let center = header_height / 2.0;
        if *open {
            commands.push(DrawCommand::Line {
                x1: 6.0,
                y1: center - 3.0,
                x2: 10.0,
                y2: center + 1.0,
                width: 2.0,
                color: theme.muted,
            });
            commands.push(DrawCommand::Line {
                x1: 10.0,
                y1: center + 1.0,
                x2: 14.0,
                y2: center - 3.0,
                width: 2.0,
                color: theme.muted,
            });
        } else {
            commands.push(DrawCommand::Line {
                x1: 7.0,
                y1: center - 4.0,
                x2: 11.0,
                y2: center,
                width: 2.0,
                color: theme.muted,
            });
            commands.push(DrawCommand::Line {
                x1: 11.0,
                y1: center,
                x2: 7.0,
                y2: center + 4.0,
                width: 2.0,
                color: theme.muted,
            });
        }
        if focused {
            commands.extend(focus_ring(0.0, 0.0, width, header_height, theme.accent));
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
                .padding(theme.spacing.sm)
                .gap(theme.spacing.xs)
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
    pub fn menu_button(
        &mut self,
        id: &str,
        label: &str,
        state: &mut MenuState,
        items: &[&str],
    ) -> MenuButtonState {
        let trigger = self.id(id).button(label);
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
                weight: if focused { 600 } else { 400 },
                color: text_color,
                max_width: Some((width - theme.spacing.sm * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            },
        ];
        if focused {
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
                    x: 2.0,
                    y: 3.0,
                    width,
                    height,
                    radius: Radius::all(9.0),
                    color: Color::rgba(0, 0, 0, 72),
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
        let row_height = measured_text_height(theme.font_sizes.body) + theme.spacing.xs * 2.0;
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
                if focused {
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
                    radius: Radius::all(3.0),
                    color: if selected {
                        mix_color(theme.surface, theme.accent, 0.18)
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
                weight: if selected { 600 } else { 400 },
                color: if selected {
                    theme.accent
                } else if hovered {
                    theme.foreground
                } else {
                    theme.muted
                },
                max_width: Some((row_width - theme.spacing.sm * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
            if focused {
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
        let header_height = measured_text_height(theme.font_sizes.body) + theme.spacing.xs * 2.0;
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

        let root = self.new_named_node(
            id,
            Style {
                width: Some(total_width),
                flex_direction: FlexDirection::Column,
                gap: 0.0,
                ..Style::default()
            },
        );
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
                    weight: 400,
                    color: theme.accent,
                    max_width: Some(10.0),
                    alignment: TextAlignment::Start,
                });
            }
            if focused {
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
                    radius: Radius::all(3.0),
                    color: if selected {
                        mix_color(theme.surface, theme.accent, 0.18)
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
                self.update_text(
                    cell,
                    text,
                    theme.font_sizes.body,
                    400,
                    if selected {
                        theme.foreground
                    } else {
                        theme.muted
                    },
                    Some((width - theme.spacing.sm * 2.0).max(1.0)),
                    TextAlignment::Start,
                );
                if let Some(cell) = self.nodes.get_mut(&cell) {
                    for command in &mut cell.commands {
                        if let DrawCommand::Text { x, y, .. } = command {
                            *x = theme.spacing.sm;
                            *y = theme.spacing.xs;
                        }
                    }
                }
            }
            self.pop_parent();
            if focused {
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
                        radius: Radius::all(3.0),
                        color: if selected {
                            mix_color(theme.surface, theme.accent, 0.18)
                        } else {
                            mix_color(theme.surface, theme.foreground, 0.06)
                        },
                    });
                }
                commands.push(DrawCommand::Text {
                    x: theme.spacing.sm,
                    y: theme.spacing.xs,
                    text: row.label.to_string(),
                    font_size: theme.font_sizes.body,
                    weight: if selected { 600 } else { 400 },
                    color: if selected {
                        theme.accent
                    } else if hovered {
                        theme.foreground
                    } else {
                        theme.muted
                    },
                    max_width: Some((width - theme.spacing.sm * 2.0).max(1.0)),
                    alignment: TextAlignment::Start,
                });
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
                weight: if open { 600 } else { 400 },
                color: theme.foreground,
                max_width: Some((width - 32.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
            if focused {
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

    /// Creates a row of clickable breadcrumb links.
    pub fn breadcrumbs(&mut self, id: &str, items: &[BreadcrumbItem<'_>]) -> BreadcrumbsResponse {
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let font_size = theme.font_sizes.body;
        let item_height = measured_text_height(font_size) + theme.spacing.xs * 2.0;
        let root = self.new_named_node(
            id,
            Style {
                width: Some(width),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                gap: theme.spacing.xs,
                ..Style::default()
            },
        );
        self.push_parent(root);
        let mut clicked = None;
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                self.text("/")
                    .font_size(theme.font_sizes.small)
                    .color(theme.muted);
            }
            let last = index + 1 == items.len();
            if last {
                self.text(item.label)
                    .font_size(font_size)
                    .color(theme.foreground);
                continue;
            }
            let key = format!("{id}:crumb:{index}");
            let item_width = measure_text_width(item.label, font_size) + theme.spacing.sm * 2.0;
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
            if hovered || focused {
                commands.push(DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: item_width,
                    height: item_height,
                    radius: Radius::all(4.0),
                    color: mix_color(theme.surface, theme.foreground, 0.08),
                });
            }
            commands.push(DrawCommand::Text {
                x: theme.spacing.sm,
                y: theme.spacing.xs,
                text: item.label.to_string(),
                font_size,
                weight: if focused { 600 } else { 400 },
                color: if hovered || focused {
                    theme.foreground
                } else {
                    theme.accent
                },
                max_width: Some((item_width - theme.spacing.sm * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            });
            if focused {
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
        let theme = self.theme.clone();
        let width = self.current_content_width();
        let font_size = theme.font_sizes.body;
        let item_height = measured_text_height(font_size) + theme.spacing.xs * 2.0;
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
                gap: theme.spacing.xs,
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
        self.push_parent(root);
        let mut flat_index = 0;
        for section in sections {
            self.text(section.title)
                .font_size(theme.font_sizes.small)
                .weight(700)
                .color(theme.muted)
                .max_width(width);
            for item in section.items {
                let key = format!("{id}:item:{flat_index}");
                let node = self.new_named_node(
                    &key,
                    Style {
                        width: Some(width),
                        height: Some(item_height),
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
                let mut commands = Vec::new();
                if item_selected || item_hovered {
                    commands.push(DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height: item_height,
                        radius: Radius::all(4.0),
                        color: if item_selected {
                            mix_color(theme.surface, theme.accent, 0.18)
                        } else {
                            mix_color(theme.surface, theme.foreground, 0.06)
                        },
                    });
                }
                if item_selected {
                    commands.push(DrawCommand::Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 2.0,
                        height: item_height,
                        color: theme.accent,
                    });
                }
                commands.push(DrawCommand::Text {
                    x: theme.spacing.sm,
                    y: theme.spacing.xs,
                    text: item.label.to_string(),
                    font_size,
                    weight: if item_selected { 600 } else { 400 },
                    color: if item_selected {
                        theme.accent
                    } else if item_hovered {
                        theme.foreground
                    } else {
                        theme.muted
                    },
                    max_width: Some((width - theme.spacing.sm * 2.0).max(1.0)),
                    alignment: TextAlignment::Start,
                });
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
        if focused {
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

    /// Lays out a labeled form field with description and error text.
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
                theme.warn()
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
                .color(theme.warn())
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

    /// Sets equal margin.
    pub fn margin(mut self, margin: f32) -> Self {
        self.node().style.margin = super::layout::Edges::all(margin);
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

    /// Sets main-axis distribution.
    pub fn justify(mut self, justify: JustifyContent) -> Self {
        self.node().style.justify_content = justify;
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

    /// Adds this text widget to keyboard focus traversal order.
    pub fn focusable(self) -> Self {
        let key = self.ctx.node_key(self.id);
        self.ctx.register_interaction(self.id, key.clone());
        self.ctx.register_focusable(self.id, key);
        self
    }

    fn sync(&mut self) {
        self.ctx.update_text(
            self.id,
            &self.text,
            self.font_size,
            self.weight,
            self.color,
            self.max_width,
            self.alignment,
        );
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

/// Creates a filled primary button.
pub fn button_primary(context: &mut Context, label: &str) -> ButtonState {
    context.button_primary(label)
}

/// Creates a button with a built-in icon before the label.
pub fn button_with_icon(context: &mut Context, label: &str, icon_name: &str) -> ButtonState {
    context.button_with_icon(label, icon_name)
}

/// Creates a flexible spacer.
pub fn spacer(context: &mut Context) {
    context.spacer();
}

/// Creates a text field.
pub fn text_field(context: &mut Context, id: &str, value: &mut String) -> TextFieldState {
    context.text_field(id, value)
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

/// Creates a slider.
pub fn slider(context: &mut Context, id: &str, value: &mut f32, min: f32, max: f32) -> SliderState {
    context.slider(id, value, min, max)
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

fn button_commands(
    label: &str,
    font_size: f32,
    theme: &Theme,
    hover_progress: f32,
    pressed_progress: f32,
    focused: bool,
    primary: bool,
) -> Vec<DrawCommand> {
    let size = estimated_text_size(label, font_size, None);
    let hover_progress = hover_progress.clamp(0.0, 1.0);
    let pressed_progress = pressed_progress.clamp(0.0, 1.0);
    let hover_background = if primary {
        mix_color(theme.accent, theme.on_accent, 0.12)
    } else {
        mix_color(theme.surface, theme.accent, 0.65)
    };
    let pressed_background = if primary {
        mix_color(theme.accent, theme.on_accent, 0.2)
    } else {
        theme.accent
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
    let emphasis = hover_progress.max(pressed_progress);
    let text_color = if primary {
        theme.on_accent
    } else {
        mix_color(theme.foreground, theme.on_accent, emphasis)
    };
    let horizontal = theme.spacing.md;
    let vertical = theme.spacing.sm;
    let mut commands = vec![
        DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width: size.width + horizontal * 2.0,
            height: size.height + vertical * 2.0,
            radius: Radius::all(6.0),
            color: background,
        },
        DrawCommand::Text {
            x: horizontal,
            y: vertical,
            text: label.to_string(),
            font_size,
            weight: 500,
            color: text_color,
            max_width: None,
            alignment: TextAlignment::Start,
        },
    ];
    if focused {
        commands.extend(focus_ring(
            0.0,
            0.0,
            size.width + horizontal * 2.0,
            size.height + vertical * 2.0,
            theme.accent,
        ));
    }
    commands
}

fn hsv_to_color(hue: f32, saturation: f32, value: f32) -> Color {
    let hue = if hue.is_finite() {
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
const STAR_PATH: &str = "M12 2 L14.47 8.6 L21.51 8.91 L15.99 13.3 L17.88 20.09 L12 16.2 L6.12 20.09 L8.01 13.3 L2.49 8.91 L9.53 8.6 Z";

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

fn estimated_text_size(text: &str, font_size: f32, max_width: Option<f32>) -> Size {
    let lines = if let Some(font) = default_font() {
        let measurer = RealFontMeasurer::new(&font);
        layout_text(text, font_size, max_width, &measurer)
    } else {
        layout_text(text, font_size, max_width, &ApproximateMeasurer)
    };
    Size::new(
        lines.iter().map(|line| line.width).fold(0.0, f32::max),
        lines.iter().map(|line| line.line_height).sum(),
    )
}

fn measured_text_height(font_size: f32) -> f32 {
    if let Some(font) = default_font() {
        font.line_height(font_size)
    } else {
        font_size * 1.25
    }
}

fn measure_text_width(text: &str, font_size: f32) -> f32 {
    if let Some(font) = default_font() {
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

fn scaled_rgba_commands(
    data: ImageData<'_>,
    commands: &mut Vec<DrawCommand>,
    target_width: f32,
    target_height: f32,
    circular_mask: bool,
) -> (usize, usize) {
    if !data.is_valid() {
        return (0, 0);
    }
    let columns = (target_width.round() as usize).clamp(1, 512);
    let rows = (target_height.round() as usize).clamp(1, 512);
    let cell_width = target_width / columns as f32;
    let cell_height = target_height / rows as f32;
    let source_width = data.width as usize;
    let source_height = data.height as usize;
    for row in 0..rows {
        for column in 0..columns {
            if circular_mask {
                let nx = (column as f32 + 0.5) / columns as f32 * 2.0 - 1.0;
                let ny = (row as f32 + 0.5) / rows as f32 * 2.0 - 1.0;
                if nx * nx + ny * ny > 1.0 {
                    continue;
                }
            }
            let source_x = (column * source_width / columns).min(source_width - 1);
            let source_y = (row * source_height / rows).min(source_height - 1);
            let offset = (source_y * source_width + source_x) * 4;
            let color = Color::rgba(
                data.pixels[offset],
                data.pixels[offset + 1],
                data.pixels[offset + 2],
                data.pixels[offset + 3],
            );
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
                DrawCommand::RoundedRect { color, .. } => Some(*color),
                _ => None,
            })
            .expect("button background");
        let midpoint = Animation::default().value(0.5);
        let hover = mix_color(theme.surface, theme.accent, 0.65);
        assert_eq!(
            background,
            mix_color(theme.surface, hover, midpoint),
            "hover animation should interpolate toward the themed accent"
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
                DrawCommand::RoundedRect { color, .. } => Some(*color),
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
        assert_eq!(spokes, 8);
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
        assert!(frame.display.commands.iter().any(|command| {
            matches!(command, DrawCommand::RoundedRect { color, .. } if *color == context.theme().accent)
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
