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

/// Active input-method preedit text and caret.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompositionState {
    /// Current preedit text.
    pub text: String,
    /// Caret as a byte offset inside `text`.
    pub cursor: usize,
}

/// Clipboard boundary used by text controls.
pub trait ClipboardProvider {
    /// Reads UTF-8 text. `None` means the clipboard is empty or unavailable.
    fn read_text(&mut self) -> Option<String>;
    /// Writes UTF-8 text, returning whether the platform accepted it.
    fn write_text(&mut self, text: &str) -> bool;
}

/// Deterministic in-memory clipboard used by tests and headless hosts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryClipboard {
    /// Current clipboard contents.
    pub text: String,
}

impl MemoryClipboard {
    /// Creates an empty clipboard.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ClipboardProvider for MemoryClipboard {
    fn read_text(&mut self) -> Option<String> {
        Some(self.text.clone())
    }

    fn write_text(&mut self, text: &str) -> bool {
        self.text.clear();
        self.text.push_str(text);
        true
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

    #[test]
    fn memory_clipboard_is_a_deterministic_provider() {
        let mut clipboard = MemoryClipboard::new();
        assert_eq!(clipboard.read_text(), Some(String::new()));
        assert!(clipboard.write_text("selected text"));
        assert_eq!(clipboard.read_text().as_deref(), Some("selected text"));
    }
}
