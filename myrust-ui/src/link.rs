//! Clickable text link.

use super::context::Context;

/// Result of constructing a link for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinkState {
    /// Whether the link was clicked or activated by Enter.
    pub clicked: bool,
    /// Whether the link owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the link.
    pub hovered: bool,
}

/// Creates a clickable text link.
///
/// The control does not open URLs; the caller decides what a click means.
/// Enter activates a focused link, and hover/focus paints an underline.
pub fn link(context: &mut Context, id: &str, label: &str) -> LinkState {
    context.link(id, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Key, PointerButton, UiEvent};
    use crate::layout::Size;
    use std::cell::RefCell;

    fn render(
        context: &mut Context,
        size: Size,
        input: Option<UiEvent>,
        last: &RefCell<LinkState>,
    ) -> super::super::native::ViewFrame {
        context.begin_frame(size);
        if let Some(event) = input {
            context.set_input(event);
        }
        *last.borrow_mut() = link(context, "docs", "Documentation");
        let frame = context.produce().unwrap();
        context.end_frame();
        frame
    }

    fn center(frame: &super::super::native::ViewFrame) -> crate::layout::Point {
        let region = &frame.interactions[0];
        crate::layout::Point::new(
            region.bounds.x + region.bounds.width / 2.0,
            region.bounds.y + region.bounds.height / 2.0,
        )
    }

    #[test]
    fn link_clicks_and_activates_with_enter() {
        let size = Size::new(240.0, 100.0);
        let mut context = Context::new(size);
        let last = RefCell::new(LinkState::default());
        let initial = render(&mut context, size, None, &last);
        let point = center(&initial);
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            &last,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            &last,
        );
        assert!(last.borrow().clicked);
        assert!(last.borrow().focused);

        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &last,
        );
        assert!(last.borrow().clicked);
    }
}
