//! Text input with floating prefix suggestions.

use super::context::Context;

/// Caller-owned autocomplete state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutocompleteState {
    /// Selected candidate index.
    pub selected: Option<usize>,
    /// Highlighted filtered suggestion.
    pub highlight: usize,
    /// Query for which the suggestion overlay was dismissed.
    pub dismissed: Option<String>,
}

impl AutocompleteState {
    /// Creates an empty autocomplete state.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Result of constructing an autocomplete for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutocompleteResponse {
    /// Selected candidate index after this frame.
    pub selected: Option<usize>,
    /// Whether the text or selected candidate changed.
    pub changed: bool,
    /// Whether the input owns focus.
    pub focused: bool,
    /// Whether the suggestion overlay is visible.
    pub visible: bool,
}

/// Creates a text input with caller-provided autocomplete candidates.
///
/// Suggestions are case-insensitive prefix matches shown below the input.
/// ArrowUp/ArrowDown move the highlight, Enter selects it, and a click on a
/// suggestion selects that row. The suggestion overlay does not take focus
/// away from the input.
pub fn autocomplete(
    context: &mut Context,
    id: &str,
    text: &mut String,
    state: &mut AutocompleteState,
    candidates: &[&str],
) -> AutocompleteResponse {
    context.autocomplete(id, text, state, candidates)
}

pub(crate) fn filter_suggestions(candidates: &[&str], text: &str) -> Vec<usize> {
    let query = text.trim().to_lowercase();
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            if query.is_empty() || candidate.to_lowercase().starts_with(&query) {
                Some(index)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::event::{Key, PointerButton, UiEvent};
    use crate::ui::layout::Size;
    use crate::ui::paint::DrawCommand;
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
    fn autocomplete_filters_and_selects_by_keyboard_and_click() {
        let size = Size::new(340.0, 220.0);
        let mut context = Context::new(size);
        let candidates = ["Rust", "Ruby", "Go", "Ruby on Rails"];
        let text = Rc::new(RefCell::new(String::new()));
        let state = Rc::new(RefCell::new(AutocompleteState::new()));
        let last = Rc::new(RefCell::new(AutocompleteResponse::default()));
        let view_text = Rc::clone(&text);
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut text = view_text.borrow_mut();
            let mut state = view_state.borrow_mut();
            *view_last.borrow_mut() =
                autocomplete(context, "auto", &mut text, &mut state, &candidates);
        };

        click(&mut context, size, "auto:input", &mut build);
        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "ru".to_string(),
            }),
            &mut build,
        );
        let suggestions = render(&mut context, size, None, &mut build);
        assert!(last.borrow().visible);
        assert!(suggestions
            .interactions
            .iter()
            .any(|region| region.button_id == "auto:option:0"));
        assert!(suggestions
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "Rust")));
        assert!(suggestions
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "Ruby")));
        assert!(!suggestions
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "Go")));

        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert_eq!(*text.borrow(), "Ruby");
        assert_eq!(last.borrow().selected, Some(1));
        assert!(last.borrow().changed);
        assert!(!last.borrow().visible);

        *text.borrow_mut() = "ru".to_string();
        render(&mut context, size, None, &mut build);
        assert!(last.borrow().visible);
        click(&mut context, size, "auto:option:0", &mut build);
        assert_eq!(*text.borrow(), "Rust");
        assert_eq!(last.borrow().selected, Some(0));
        assert!(last.borrow().changed);
    }
}
