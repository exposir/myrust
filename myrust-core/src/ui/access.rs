//! Platform-neutral accessibility semantics.
//!
//! These structures are metadata only. No AT-SPI, UIA, or NSAccessibility
//! bridge is connected yet, so platform assistive technologies cannot observe
//! this tree.

use super::paint::Rect;

/// Accessibility role for a widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Action button.
    Button,
    /// Boolean checkbox.
    CheckBox,
    /// Numeric slider.
    Slider,
    /// Single-line text field.
    TextField,
    /// Grouping container.
    Group,
    /// Static label.
    Label,
    /// Scrollable region.
    ScrollView,
}

/// One frame's accessibility node.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessNode {
    /// Semantic role.
    pub role: Role,
    /// Accessible label.
    pub label: String,
    /// Optional textual value/state.
    pub value: Option<String>,
    /// Whether the widget is disabled.
    pub disabled: bool,
    /// Whether the widget currently has keyboard focus.
    pub focused: bool,
    /// Logical screen-space bounds.
    pub bounds: Rect,
}
