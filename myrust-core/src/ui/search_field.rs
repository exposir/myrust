//! Single-line search field with clear and submit semantics.

use super::context::Context;

/// Result of constructing a search field for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchFieldState {
    /// Whether the text changed (including a clear).
    pub changed: bool,
    /// Whether Enter was pressed while the input was focused.
    pub submitted: bool,
    /// Whether Escape or the clear button cleared the text.
    pub cleared: bool,
    /// Whether the input owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the input.
    pub hovered: bool,
    /// Current text after this frame's input.
    pub text: String,
}

/// Creates a search field with a clear button.
///
/// The field reuses the normal text-field editor. Enter reports `submitted`
/// without clearing; Escape and the trailing clear button clear the text.
pub fn search_field(context: &mut Context, id: &str, text: &mut String) -> SearchFieldState {
    context.search_field(id, text)
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
    fn search_field_types_clears_and_submits() {
        let size = Size::new(320.0, 90.0);
        let mut context = Context::new(size);
        let text = Rc::new(RefCell::new(String::new()));
        let last = Rc::new(RefCell::new(SearchFieldState::default()));
        let view_text = Rc::clone(&text);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut text = view_text.borrow_mut();
            *view_last.borrow_mut() = search_field(context, "search", &mut text);
        };

        click(&mut context, size, "search:input", &mut build);
        assert!(last.borrow().focused);
        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "query".to_string(),
            }),
            &mut build,
        );
        assert_eq!(*text.borrow(), "query");
        assert!(last.borrow().changed);
        assert_eq!(last.borrow().text, "query");

        click(&mut context, size, "search:clear", &mut build);
        assert!(text.borrow().is_empty());
        assert!(last.borrow().cleared);
        assert!(last.borrow().focused);

        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "again".to_string(),
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert!(last.borrow().submitted);
        assert_eq!(*text.borrow(), "again");
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert!(text.borrow().is_empty());
        assert!(last.borrow().cleared);
        render(&mut context, size, None, &mut build);
        assert_eq!(last.borrow().text, "");
        assert_eq!(*text.borrow(), "");
    }
}
