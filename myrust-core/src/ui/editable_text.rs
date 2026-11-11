//! In-place editable label.

use super::context::Context;

/// Caller-owned editable text state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditableTextState {
    /// Whether the field is in edit mode.
    pub editing: bool,
    /// Edit buffer, valid while editing.
    pub buffer: String,
    pub(crate) was_focused: bool,
}

impl EditableTextState {
    /// Starts in view mode.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Result of constructing an editable text for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditableTextResponse {
    /// Whether the committed text changed.
    pub changed: bool,
    /// Whether Enter or blur committed the edit.
    pub submitted: bool,
    /// Whether Escape discarded the edit.
    pub cancelled: bool,
    /// Whether the control is still editing.
    pub editing: bool,
    /// Whether the input or view owns focus.
    pub focused: bool,
}

/// Creates a single-line label that becomes editable when clicked.
///
/// Enter commits, Escape cancels, and losing focus commits (unless the edit is
/// empty, in which case it is treated as a cancellation).
pub fn editable_text(
    context: &mut Context,
    id: &str,
    state: &mut EditableTextState,
    text: &mut String,
) -> EditableTextResponse {
    context.editable_text(id, state, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::event::{Key, PointerButton, UiEvent};
    use crate::ui::layout::Size;
    use std::{cell::RefCell, rc::Rc};

    fn render(
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

    fn center_of(frame: &super::super::native::ViewFrame, id: &str) -> crate::ui::layout::Point {
        let region = frame
            .interactions
            .iter()
            .find(|region| region.button_id == id)
            .unwrap_or_else(|| panic!("missing interaction region {id}"));
        crate::ui::layout::Point::new(
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
        let initial = render(context, size, None, build);
        let point = center_of(&initial, id);
        render(
            context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            build,
        );
        render(
            context,
            size,
            Some(UiEvent::PointerReleased {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            build,
        )
    }

    #[test]
    fn editable_text_edits_commits_cancels_and_commits_on_blur() {
        let size = Size::new(280.0, 120.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(EditableTextState::new()));
        let text = Rc::new(RefCell::new("Old".to_string()));
        let last = Rc::new(RefCell::new(EditableTextResponse::default()));
        let view_state = Rc::clone(&state);
        let view_text = Rc::clone(&text);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            let mut text = view_text.borrow_mut();
            *view_last.borrow_mut() = editable_text(context, "rename", &mut state, &mut text);
        };

        click(&mut context, size, "rename", &mut build);
        assert!(state.borrow().editing);
        assert!(last.borrow().focused);
        for _ in 0..3 {
            render(
                &mut context,
                size,
                Some(UiEvent::Key {
                    key: Key::Backspace,
                }),
                &mut build,
            );
        }
        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "New".to_string(),
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert_eq!(*text.borrow(), "New");
        assert!(!state.borrow().editing);
        assert!(last.borrow().submitted);
        assert!(last.borrow().changed);

        click(&mut context, size, "rename", &mut build);
        state.borrow_mut().buffer = "Canceled".to_string();
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert_eq!(*text.borrow(), "New");
        assert!(!state.borrow().editing);
        assert!(last.borrow().cancelled);
        assert!(!last.borrow().changed);

        click(&mut context, size, "rename", &mut build);
        state.borrow_mut().buffer = "Blurred".to_string();
        context.request_focus("other");
        render(&mut context, size, None, &mut build);
        assert_eq!(*text.borrow(), "Blurred");
        assert!(!state.borrow().editing);
        assert!(last.borrow().submitted);
        assert!(last.borrow().changed);
    }
}
