//! Immediate-mode UI context and widget builders.
//!
//! Widgets are constructed directly on [`Context`] during every frame. Layout
//! nodes and drawing commands are collected first, then resolved into absolute
//! coordinates after the user callback returns.
//!
//! Focus order follows widget construction order. Explicit ids must be unique;
//! duplicate ids share interaction, focus, and persistent widget state.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use super::access::{AccessNode, Role};
use super::animation::{Animation, Easing};
use super::event::{resolve_button_regions, InteractiveRegion, Key, UiEvent};
use super::font::{default_font, RealFontMeasurer};
use super::layout::{
    AlignItems, FlexDirection, JustifyContent, LayoutEngine, LayoutResult, Node, Point, Size, Style,
};
use super::paint::{Color, DisplayList, DrawCommand, Radius, Rect, TextAlignment, Transform};
use super::render::{CommandCounts, RenderStats};
use super::text::{layout_text, ApproximateMeasurer};
use super::theme::Theme;
use crate::desktop::system::AppearancePreferences;

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
    wheel: (f32, f32),
    text: String,
    keys: Vec<Key>,
}

enum WidgetState {
    TextField { cursor: usize },
    Checkbox,
    Slider { dragging: bool },
    ScrollView { offset: f32, dragging: bool },
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
    pending_focusables: Vec<(u64, String)>,
    current_focus_key: Option<String>,
    widget_states: HashMap<String, WidgetState>,
    theme: Theme,
    animations: HashMap<String, AnimatedValue>,
    frame_time: Instant,
    frame_serial: u64,
    reduce_motion: bool,
    pending_access_nodes: Vec<PendingAccessNode>,
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
            frame_serial: 0,
            reduce_motion: detect_reduce_motion(),
            pending_access_nodes: Vec::new(),
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
        let Some(pointer) = self.input.pointer else {
            return false;
        };
        self.previous_regions
            .get(id_hint)
            .map(|region| region.contains(pointer.x, pointer.y))
            .unwrap_or(false)
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
        let mut builder = TextBuilder {
            ctx: self,
            id,
            text: content.to_string(),
            font_size,
            weight: 400,
            color,
            max_width: None,
            alignment: TextAlignment::Start,
        };
        builder.sync();
        builder
    }

    /// Creates a button and returns its click state for this frame.
    pub fn button(&mut self, label: &str) -> ButtonState {
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
        let released = self.input.released;
        let mouse_clicked = previous
            .as_ref()
            .zip(released)
            .map(|(region, point)| region.contains(point.x, point.y))
            .unwrap_or(false);
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
    pub fn text_field(&mut self, id: &str, value: &mut String) -> TextFieldState {
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
        let mut cursor = match self.widget_states.get(id) {
            Some(WidgetState::TextField { cursor }) => *cursor,
            _ => 0,
        };
        cursor = cursor.min(value.len());
        while !value.is_char_boundary(cursor) {
            cursor -= 1;
        }
        if focused {
            let text = self.input.text.clone();
            if !text.is_empty() {
                value.insert_str(cursor, &text);
                cursor += text.len();
                changed = true;
            }
            for key in self.input.keys.clone() {
                match key {
                    Key::Backspace => {
                        if let Some(previous) = previous_char_boundary(value, cursor) {
                            value.replace_range(previous..cursor, "");
                            cursor = previous;
                            changed = true;
                        }
                    }
                    Key::Delete => {
                        if let Some(next) = next_char_boundary(value, cursor) {
                            value.replace_range(cursor..next, "");
                            changed = true;
                        }
                    }
                    Key::ArrowLeft => {
                        if let Some(previous) = previous_char_boundary(value, cursor) {
                            cursor = previous;
                        }
                    }
                    Key::ArrowRight => {
                        if let Some(next) = next_char_boundary(value, cursor) {
                            cursor = next;
                        }
                    }
                    Key::Home => cursor = 0,
                    Key::End => cursor = value.len(),
                    _ => {}
                }
            }
        }
        if let Some(WidgetState::TextField { cursor: stored }) = self.widget_states.get_mut(id) {
            *stored = cursor;
        }
        let height = measured_text_height(font_size) + vertical * 2.0;
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
                color: theme.surface,
            },
            DrawCommand::Text {
                x: horizontal,
                y: vertical,
                text: value.clone(),
                font_size,
                weight: 400,
                color: theme.foreground,
                max_width: Some((width - horizontal * 2.0).max(1.0)),
                alignment: TextAlignment::Start,
            },
        ];
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

    /// Creates a checkbox that toggles by mouse or focused Enter/Space.
    pub fn checkbox(&mut self, id: &str, checked: &mut bool) -> CheckboxState {
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
        let mouse_clicked = self
            .input
            .released
            .and_then(|point| {
                self.previous_regions
                    .get(id)
                    .map(|region| region.contains(point.x, point.y))
            })
            .unwrap_or(false);
        let keyboard_clicked =
            focused && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space));
        let changed = mouse_clicked || keyboard_clicked;
        if changed {
            *checked = !*checked;
        }
        let mut commands = vec![
            DrawCommand::RoundedRect {
                x: 0.0,
                y: (height - 18.0) / 2.0,
                width: 18.0,
                height: 18.0,
                radius: Radius::all(4.0),
                color: if *checked {
                    theme.accent
                } else {
                    theme.surface
                },
            },
            DrawCommand::Text {
                x: 26.0,
                y: (height - measured_text_height(font_size)) / 2.0,
                text: if *checked { "checked" } else { "unchecked" }.to_string(),
                font_size,
                weight: 400,
                color: theme.foreground,
                max_width: None,
                alignment: TextAlignment::Start,
            },
        ];
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
        let (dragging, changed) = match self.widget_states.get_mut(id) {
            Some(WidgetState::Slider { dragging }) => {
                if pointer_pressed {
                    *dragging = true;
                }
                let mut changed = false;
                if *dragging {
                    if let (Some(pointer), Some(region)) =
                        (self.input.pointer, self.previous_regions.get(id))
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
        let mut changed = false;
        let (offset, dragging) = match self.widget_states.get_mut(id) {
            Some(WidgetState::ScrollView { offset, dragging }) => {
                if hovered && self.input.wheel.1.abs() > f32::EPSILON {
                    *offset = (*offset + self.input.wheel.1).clamp(0.0, max_scroll);
                    changed = true;
                }
                if hovered && self.input.pointer_down {
                    if let (Some(pointer), Some(region)) =
                        (self.input.pointer, self.previous_regions.get(id))
                    {
                        if pointer.x >= region.bounds.right() - 10.0 {
                            *dragging = true;
                        }
                    }
                }
                if *dragging {
                    if let (Some(pointer), Some(region)) =
                        (self.input.pointer, self.previous_regions.get(id))
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
        self.frame_time = Instant::now();
        self.frame_serial = self.frame_serial.saturating_add(1);
        let current_frame = self.frame_serial;
        self.animations
            .retain(|_, animation| animation.last_seen_frame.saturating_add(1) >= current_frame);
        self.previous_regions = std::mem::take(&mut self.current_regions);
        self.current_regions.clear();
        self.nodes.clear();
        self.roots.clear();
        self.parent_stack.clear();
        self.explicit_id = None;
        self.pending_interactions.clear();
        self.pending_focusables.clear();
        self.current_focus_key = None;
        self.pending_access_nodes.clear();
        self.next_id = 1;
    }

    pub(crate) fn set_input(&mut self, event: UiEvent) {
        match event {
            UiEvent::PointerMoved { x, y } => {
                self.input.pointer = Some(Point::new(x, y));
            }
            UiEvent::PointerPressed { x, y, .. } => {
                self.input.pointer = Some(Point::new(x, y));
                self.input.pointer_down = true;
                self.input.released = None;
            }
            UiEvent::PointerReleased { x, y, .. } => {
                self.input.pointer = Some(Point::new(x, y));
                self.input.pointer_down = false;
                self.input.released = Some(Point::new(x, y));
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
        self.frame_time += duration;
    }

    pub(crate) fn needs_animation_frame(&self) -> bool {
        self.animations
            .values()
            .any(|animation| animation.active && animation.last_seen_frame == self.frame_serial)
    }

    pub(crate) fn produce(&mut self) -> Result<super::native::ViewFrame, String> {
        let layout = self.compute_layout();
        let mut display = DisplayList::new();
        for root in &self.roots {
            self.append_display(*root, &layout, &mut display);
        }
        let interactions = resolve_button_regions(&layout, &self.pending_interactions);
        self.current_regions = interactions
            .iter()
            .map(|region| (region.button_id.clone(), region.clone()))
            .collect();
        self.focus_order = self
            .pending_focusables
            .iter()
            .filter(|(node_id, _)| layout.iter().any(|result| result.id == *node_id))
            .map(|(_, key)| key.clone())
            .collect();
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
        let root = if self.roots.len() == 1 {
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
        Some(node)
    }

    fn append_display(&self, id: u64, layout: &[LayoutResult], display: &mut DisplayList) {
        let Some(pending) = self.nodes.get(&id) else {
            return;
        };
        let result = layout.iter().find(|result| result.id == id).copied();
        if let Some(result) = result {
            let mut local = DisplayList::from_commands(pending.commands.clone());
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
            self.widget_states
                .insert(id.to_string(), WidgetState::TextField { cursor: 0 });
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
        self.pending_focusables.push((node_id, key.clone()));
        self.current_focus_key = Some(key);
    }

    fn register_interaction(&mut self, node_id: u64, key: String) {
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

    fn pointer_in_previous(&self, id: &str) -> bool {
        self.input
            .pointer
            .and_then(|pointer| {
                self.previous_regions
                    .get(id)
                    .map(|region| region.contains(pointer.x, pointer.y))
            })
            .unwrap_or(false)
    }
}

struct PendingNode {
    style: Style,
    content_size: Size,
    children: Vec<u64>,
    commands: Vec<DrawCommand>,
    overlay_commands: Vec<DrawCommand>,
    scroll: Option<ScrollNode>,
    key: Option<String>,
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

/// Creates a vertical flex container.
pub fn column(context: &mut Context) -> ColumnBuilder<'_> {
    context.column()
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
) -> Vec<DrawCommand> {
    let size = estimated_text_size(label, font_size, None);
    let hover_progress = hover_progress.clamp(0.0, 1.0);
    let pressed_progress = pressed_progress.clamp(0.0, 1.0);
    let hover_background = mix_color(theme.surface, theme.accent, 0.65);
    let pressed_background = theme.accent;
    let background = mix_color(
        mix_color(theme.surface, hover_background, hover_progress),
        pressed_background,
        pressed_progress,
    );
    let emphasis = hover_progress.max(pressed_progress);
    let text_color = mix_color(theme.foreground, theme.on_accent, emphasis);
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
    AppearancePreferences::detect()
        .reduced_motion
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            button: crate::ui::event::PointerButton::Left,
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
            button: crate::ui::event::PointerButton::Left,
        });
        context.checkbox("toggle", &mut checked);
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(300.0, 120.0));
        context.set_input(UiEvent::PointerReleased {
            x,
            y,
            button: crate::ui::event::PointerButton::Left,
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
            button: crate::ui::event::PointerButton::Left,
        });
        context.slider("level", &mut value, 0.0, 100.0);
        assert!((value - 50.0).abs() <= 5.0, "midpoint value was {value}");
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(Size::new(300.0, 120.0));
        context.set_input(UiEvent::PointerReleased {
            x,
            y,
            button: crate::ui::event::PointerButton::Left,
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
            button: crate::ui::event::PointerButton::Left,
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
}
