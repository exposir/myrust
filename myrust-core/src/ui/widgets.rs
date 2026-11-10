//! Immediate-mode widget builder re-exports.
//!
//! Widget construction now lives on [`crate::ui::Context`]. This module keeps
//! the short `ui::widgets` import path available without retaining any
//! retained-mode widget trait or stateful view type.

pub use super::context::{
    button, checkbox, column, container, row, scroll_view, slider, spacer, text, text_field,
    ButtonState, CheckboxState, ColumnBuilder, ContainerBuilder, FlexBuilder, RowBuilder,
    ScrollState, SliderState, TextBuilder, TextFieldState,
};
