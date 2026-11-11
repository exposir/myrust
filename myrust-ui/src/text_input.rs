//! Text-input state and geometry primitives.

use super::layout::Point;
use super::paint::Rect;

/// Queryable text-input state for custom controls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextInputState {
    /// Cursor byte offset.
    pub cursor: usize,
    /// Selected byte range, normalized as `(start, end)`.
    pub selection: Option<(usize, usize)>,
    /// Field bounds from the previous frame.
    pub bounds: Rect,
    /// Absolute cursor x position.
    pub cursor_x: f32,
    /// Absolute cursor y position.
    pub cursor_y: f32,
}

impl TextInputState {
    /// Returns the cursor position as a point.
    pub fn cursor_position(self) -> Point {
        Point::new(self.cursor_x, self.cursor_y)
    }

    /// Returns the normalized selected range.
    pub fn selection_range(self) -> Option<std::ops::Range<usize>> {
        self.selection.map(|(start, end)| start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::event::UiEvent;
    use crate::layout::Size;

    #[test]
    fn text_input_state_reports_cursor_selection_and_geometry() {
        let size = Size::new(260.0, 100.0);
        let mut context = Context::new(size);
        let mut value = String::new();
        context.begin_frame(size);
        context.request_focus("field");
        context.set_input(UiEvent::TextInput {
            text: "hello".to_string(),
        });
        context.text_field("field", &mut value);
        let frame = context.produce().unwrap();
        context.end_frame();
        assert!(frame
            .interactions
            .iter()
            .any(|region| region.button_id == "field"));
        let state = context.text_input_state("field").expect("text field state");
        assert_eq!(state.cursor, 5);
        assert_eq!(state.selection, None);
        assert!(state.cursor_x >= state.bounds.x);
        assert!(state.cursor_y >= state.bounds.y);
        assert!(state.cursor_x <= state.bounds.right());
        assert!(state.cursor_y <= state.bounds.bottom());
        assert_eq!(state.selection_range(), None);
        assert_eq!(
            state.cursor_position(),
            Point::new(state.cursor_x, state.cursor_y)
        );

        context.set_text_input_selection("field", 1, 3);
        let selected = context.text_input_state("field").unwrap();
        assert_eq!(selected.selection, Some((1, 3)));
        context.begin_frame(size);
        context.set_input(UiEvent::TextInput {
            text: "X".to_string(),
        });
        let field = context.text_field("field", &mut value);
        assert_eq!(value, "hXlo");
        assert_eq!(field.cursor, 2);
        let after = context.text_input_state("field").unwrap();
        assert_eq!(after.selection, None);
        context.produce().unwrap();
        context.end_frame();
    }
}
