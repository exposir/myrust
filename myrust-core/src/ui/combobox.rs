//! Editable combobox with a filtered overlay list.

use super::context::Context;

/// Caller-owned combobox state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComboboxState {
    /// Whether the option list is open.
    pub open: bool,
    /// Selected option index.
    pub selected: Option<usize>,
    /// Highlighted filtered option.
    pub highlight: usize,
}

impl ComboboxState {
    /// Creates a closed combobox state.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Result of constructing a combobox for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComboboxResponse {
    /// Whether the list remains open.
    pub open: bool,
    /// Selected option index after this frame's input.
    pub selected: Option<usize>,
    /// Highlighted filtered option.
    pub highlight: usize,
    /// Whether the query or selection changed.
    pub changed: bool,
    /// Whether the text input owns focus.
    pub focused: bool,
}

/// Creates an editable combobox.
///
/// Typing filters the options case-insensitively. ArrowUp/ArrowDown move the
/// highlight, Enter selects it, and Escape closes the overlay. The option
/// overlay is built with the shared overlay primitive but its rows are not
/// focusable, so keyboard focus stays in the text input.
pub fn combobox(
    context: &mut Context,
    id: &str,
    query: &mut String,
    state: &mut ComboboxState,
    options: &[&str],
) -> ComboboxResponse {
    context.combobox(id, query, state, options)
}

pub(crate) fn filter_options(options: &[&str], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    options
        .iter()
        .enumerate()
        .filter_map(|(index, option)| {
            if query.is_empty() || option.to_lowercase().contains(&query) {
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
    fn combobox_filters_and_selects_with_keyboard() {
        let size = Size::new(340.0, 220.0);
        let mut context = Context::new(size);
        let options = ["Rust", "Ruby", "Go", "JavaScript"];
        let query = Rc::new(RefCell::new(String::new()));
        let state = Rc::new(RefCell::new(ComboboxState::new()));
        let last = Rc::new(RefCell::new(ComboboxResponse::default()));
        let view_query = Rc::clone(&query);
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut query = view_query.borrow_mut();
            let mut state = view_state.borrow_mut();
            *view_last.borrow_mut() = combobox(context, "combo", &mut query, &mut state, &options);
        };

        click(&mut context, size, "combo:input", &mut build);
        assert!(last.borrow().open);
        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "ru".to_string(),
            }),
            &mut build,
        );
        assert_eq!(*query.borrow(), "ru");
        let filtered = render(&mut context, size, None, &mut build);
        assert!(filtered
            .interactions
            .iter()
            .any(|region| region.button_id == "combo:option:0"));
        assert!(filtered
            .interactions
            .iter()
            .any(|region| region.button_id == "combo:option:1"));
        assert!(filtered
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "Rust")));
        assert!(filtered
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "Ruby")));
        assert!(!filtered
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
        assert_eq!(last.borrow().highlight, 1);
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert_eq!(*query.borrow(), "Ruby");
        assert_eq!(last.borrow().selected, Some(1));
        assert!(!last.borrow().open);
        assert!(last.borrow().changed);
        assert_eq!(state.borrow().selected, Some(1));

        click(&mut context, size, "combo:input", &mut build);
        assert!(last.borrow().open);
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert!(!last.borrow().open);
    }
}
