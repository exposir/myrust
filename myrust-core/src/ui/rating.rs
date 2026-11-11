//! Star rating control.

use super::context::Context;

/// Result of constructing a rating control for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RatingState {
    /// Whether the value changed this frame.
    pub changed: bool,
    /// Value after this frame's input.
    pub value: u32,
    /// Whether the rating owns keyboard focus.
    pub focused: bool,
    /// Hover preview value, if the pointer is over a star.
    pub hover_preview: Option<u32>,
}

/// Creates a star rating from zero to `max`.
///
/// Clicking star `n` sets the value to `n`. Hovering previews that value.
/// While focused, ArrowLeft/ArrowDown decrement, ArrowRight/ArrowUp increment,
/// Home clears, and End selects `max`.
pub fn rating(context: &mut Context, id: &str, value: &mut u32, max: u32) -> RatingState {
    context.rating(id, value, max)
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
    fn rating_click_sets_value_and_keyboard_and_hover_preview_work() {
        let size = Size::new(220.0, 100.0);
        let mut context = Context::new(size);
        let value = Rc::new(RefCell::new(2_u32));
        let last = Rc::new(RefCell::new(RatingState::default()));
        let view_value = Rc::clone(&value);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut value = view_value.borrow_mut();
            *view_last.borrow_mut() = rating(context, "rating", &mut value, 5);
        };

        click(&mut context, size, "rating:star:2", &mut build);
        assert_eq!(*value.borrow(), 3);
        assert!(last.borrow().changed);
        assert!(last.borrow().focused);

        let star_four = center_of(
            &render(&mut context, size, None, &mut build),
            "rating:star:3",
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: star_four.x,
                y: star_four.y,
            }),
            &mut build,
        );
        assert_eq!(last.borrow().hover_preview, Some(4));

        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::ArrowUp }),
            &mut build,
        );
        assert_eq!(*value.borrow(), 4);
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        assert_eq!(*value.borrow(), 3);
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Home }),
            &mut build,
        );
        assert_eq!(*value.borrow(), 0);
    }
}
