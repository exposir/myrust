//! Batch 1 native controls built on the immediate-mode context.
//!
//! Each control follows the existing split: the caller owns the value
//! (`&mut bool`, `&mut usize`, `&mut f32`, `&mut String`), the context owns
//! transient interaction state keyed by the stable id, and a small state
//! struct reports this frame's result.

use super::context::Context;

/// Result of constructing a switch for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchState {
    /// Whether the value changed this frame.
    pub changed: bool,
    /// Value after applying this frame's input.
    pub checked: bool,
    /// Whether the switch owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the switch.
    pub hovered: bool,
}

/// Result of constructing a radio group for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioGroupState {
    /// Whether the selected option changed this frame.
    pub changed: bool,
    /// Selected option index after applying this frame's input.
    pub selected: usize,
    /// Whether the group owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over any option in the group.
    pub hovered: bool,
}

/// Result of constructing a select control for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectState {
    /// Whether the selected option changed this frame.
    pub changed: bool,
    /// Whether the drop-down is open.
    pub open: bool,
    /// Selected option index after applying this frame's input.
    pub selected: usize,
    /// Whether the trigger owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the trigger.
    pub hovered: bool,
}

/// Result of constructing a tab row for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabsState {
    /// Whether the selected tab changed this frame.
    pub changed: bool,
    /// Selected tab index after applying this frame's input.
    pub selected: usize,
    /// Whether the tab row owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over any tab.
    pub hovered: bool,
}

/// Result of constructing a progress bar for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgressState {
    /// Clamped value in `0.0..=max`.
    pub value: f32,
    /// Normalized progress in `0.0..=1.0`.
    pub fraction: f32,
    /// Whether the bar shows an unknown-duration activity.
    pub indeterminate: bool,
}

/// Result of constructing a spinner for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpinnerState {
    /// Current rotation angle in radians.
    pub angle: f32,
    /// Whether another frame is requested to keep the spinner moving.
    pub spinning: bool,
}

/// Result of constructing a badge for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BadgeState {
    /// Measured badge width.
    pub width: f32,
    /// Measured badge height.
    pub height: f32,
}

/// Result of constructing a collapsible section for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollapsibleState {
    /// Whether the open flag changed this frame.
    pub changed: bool,
    /// Whether the section is open.
    pub open: bool,
    /// Whether the header owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the header.
    pub hovered: bool,
}

/// Creates a switch that toggles by mouse or focused Enter/Space.
pub fn switch(context: &mut Context, id: &str, checked: &mut bool) -> SwitchState {
    context.switch(id, checked)
}

/// Creates a radio group that selects one option by mouse or arrow keys.
pub fn radio_group(
    context: &mut Context,
    id: &str,
    selected: &mut usize,
    options: &[&str],
) -> RadioGroupState {
    context.radio_group(id, selected, options)
}

/// Creates a drop-down select anchored to its trigger.
pub fn select(
    context: &mut Context,
    id: &str,
    selected: &mut usize,
    options: &[&str],
) -> SelectState {
    context.select(id, selected, options)
}

/// Creates a row of tabs choosing one index.
pub fn tabs(context: &mut Context, id: &str, selected: &mut usize, labels: &[&str]) -> TabsState {
    context.tabs(id, selected, labels)
}

/// Creates a determinate progress bar.
pub fn progress(context: &mut Context, id: &str, value: f32, max: f32) -> ProgressState {
    context.progress(id, value, max)
}

/// Creates an indeterminate progress bar that requests animation frames.
pub fn progress_indeterminate(context: &mut Context, id: &str) -> ProgressState {
    context.progress_indeterminate(id)
}

/// Creates a spinner that requests animation frames.
pub fn spinner(context: &mut Context, id: &str) -> SpinnerState {
    context.spinner(id)
}

/// Creates a short text pill.
pub fn badge(context: &mut Context, label: &str) -> BadgeState {
    context.badge(label)
}

/// Creates a header that shows or hides its content.
pub fn collapsible(
    context: &mut Context,
    id: &str,
    open: &mut bool,
    title: &str,
    content: impl FnOnce(&mut Context),
) -> CollapsibleState {
    context.collapsible(id, open, title, content)
}
