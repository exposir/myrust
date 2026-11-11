//! Token/chip text field.

use super::context::Context;

/// Result of constructing a token field for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenFieldState {
    /// Whether the token list or input text changed.
    pub changed: bool,
    /// Whether at least one token was added.
    pub added: bool,
    /// Whether at least one token was removed.
    pub removed: bool,
    /// Whether the input owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the input.
    pub hovered: bool,
}

/// Creates a chip-style token field.
///
/// Enter adds the buffered text as a token. A comma splits buffered text into
/// completed tokens. Backspace on an empty buffer removes the last token.
/// Clicking a chip removes that token.
pub fn token_field(
    context: &mut Context,
    id: &str,
    tokens: &mut Vec<String>,
    input: &mut String,
) -> TokenFieldState {
    context.token_field(id, tokens, input)
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
    fn token_field_adds_by_enter_and_comma_and_removes_by_backspace_and_click() {
        let size = Size::new(360.0, 180.0);
        let mut context = Context::new(size);
        let tokens = Rc::new(RefCell::new(Vec::new()));
        let input = Rc::new(RefCell::new(String::new()));
        let last = Rc::new(RefCell::new(TokenFieldState::default()));
        let view_tokens = Rc::clone(&tokens);
        let view_input = Rc::clone(&input);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut tokens = view_tokens.borrow_mut();
            let mut input = view_input.borrow_mut();
            *view_last.borrow_mut() = token_field(context, "tokens", &mut tokens, &mut input);
        };

        click(&mut context, size, "tokens:input", &mut build);
        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "alpha".to_string(),
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert_eq!(*tokens.borrow(), vec!["alpha".to_string()]);
        assert!(input.borrow().is_empty());
        assert!(last.borrow().added);

        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "beta,".to_string(),
            }),
            &mut build,
        );
        assert_eq!(
            *tokens.borrow(),
            vec!["alpha".to_string(), "beta".to_string()]
        );
        assert!(input.borrow().is_empty());
        assert!(last.borrow().added);

        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::Backspace,
            }),
            &mut build,
        );
        assert_eq!(*tokens.borrow(), vec!["alpha".to_string()]);
        assert!(last.borrow().removed);

        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "gamma".to_string(),
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert_eq!(
            *tokens.borrow(),
            vec!["alpha".to_string(), "gamma".to_string()]
        );

        click(&mut context, size, "tokens:token:0", &mut build);
        assert_eq!(*tokens.borrow(), vec!["gamma".to_string()]);
        assert!(last.borrow().removed);
    }
}
