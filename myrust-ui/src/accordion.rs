//! Expandable accordion sections.

use super::context::Context;

/// Whether one or many sections may stay open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccordionMode {
    /// Opening a section closes any other open section.
    Single,
    /// Multiple sections may stay open at once.
    #[default]
    Multiple,
}

/// Caller-owned accordion state.
#[derive(Debug, Clone, Default)]
pub struct AccordionState {
    /// Ids of sections currently open.
    pub open: Vec<String>,
    /// Single-open or multi-open behavior.
    pub mode: AccordionMode,
}

impl AccordionState {
    /// Creates an empty state with an explicit mode.
    pub fn new(mode: AccordionMode) -> Self {
        Self {
            open: Vec::new(),
            mode,
        }
    }

    /// Returns whether a section id is open.
    pub fn is_open(&self, id: &str) -> bool {
        self.open.iter().any(|open| open == id)
    }
}

/// One accordion section. Content is text; use composition outside the
/// accordion when a section needs arbitrary widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccordionSection<'a> {
    /// Stable section id.
    pub id: &'a str,
    /// Header title.
    pub title: &'a str,
    /// Body text shown while open.
    pub content: &'a str,
}

impl<'a> AccordionSection<'a> {
    /// Creates an accordion section.
    pub const fn new(id: &'a str, title: &'a str, content: &'a str) -> Self {
        Self { id, title, content }
    }
}

/// Result of constructing an accordion for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccordionResponse {
    /// Section toggled this frame, if any.
    pub toggled_id: Option<String>,
    /// Whether any section open state changed.
    pub changed: bool,
}

/// Creates a set of expandable sections.
///
/// Headers toggle by click or by focused Enter/Space. A single-open state
/// clears other ids when a new section opens; a multi-open state toggles only
/// the requested id.
pub fn accordion(
    context: &mut Context,
    id: &str,
    state: &mut AccordionState,
    sections: &[AccordionSection<'_>],
) -> AccordionResponse {
    context.accordion(id, state, sections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Role;
    use crate::event::{Key, PointerButton, UiEvent};
    use crate::layout::Size;
    use crate::paint::DrawCommand;
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

    fn center_of(frame: &super::super::native::ViewFrame, id: &str) -> crate::layout::Point {
        let region = frame
            .interactions
            .iter()
            .find(|region| region.button_id == id)
            .unwrap_or_else(|| panic!("missing interaction region {id}"));
        crate::layout::Point::new(
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
    fn accordion_toggles_keyboard_and_enforces_single_open_mode() {
        let size = Size::new(420.0, 300.0);
        let mut context = Context::new(size);
        let sections = [
            AccordionSection::new("a", "Alpha", "Alpha body"),
            AccordionSection::new("b", "Beta", "Beta body"),
            AccordionSection::new("c", "Gamma", "Gamma body"),
        ];
        let state = Rc::new(RefCell::new(AccordionState::new(AccordionMode::Single)));
        let last = Rc::new(RefCell::new(AccordionResponse::default()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            *view_last.borrow_mut() = accordion(context, "acc", &mut state, &sections);
        };

        let opened_a = click(&mut context, size, "acc:header:a", &mut build);
        assert_eq!(last.borrow().toggled_id.as_deref(), Some("a"));
        assert_eq!(state.borrow().open, vec!["a".to_string()]);
        assert!(opened_a.display.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text == "Alpha body")
        ));
        assert!(opened_a.access_nodes.iter().any(|node| {
            node.role == Role::Disclosure
                && node.label == "Alpha"
                && node.value.as_deref() == Some("expanded")
        }));

        let opened_b = click(&mut context, size, "acc:header:b", &mut build);
        assert_eq!(state.borrow().open, vec!["b".to_string()]);
        assert!(!opened_b.display.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text == "Alpha body")
        ));
        assert!(opened_b.display.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text == "Beta body")
        ));

        state.borrow_mut().mode = AccordionMode::Multiple;
        state.borrow_mut().open.clear();
        click(&mut context, size, "acc:header:a", &mut build);
        click(&mut context, size, "acc:header:b", &mut build);
        assert_eq!(state.borrow().open, vec!["a".to_string(), "b".to_string()]);

        context.request_focus("acc:header:b");
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Space }),
            &mut build,
        );
        assert_eq!(state.borrow().open, vec!["a".to_string()]);
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert_eq!(state.borrow().open, vec!["a".to_string(), "b".to_string()]);
    }
}
