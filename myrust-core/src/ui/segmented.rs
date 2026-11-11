//! Horizontal segmented control.

use super::context::Context;

/// Result of constructing a segmented control.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SegmentedResponse {
    /// Selected index after this frame's input.
    pub selected: usize,
    /// Whether the selection changed.
    pub changed: bool,
    /// Segment clicked this frame, if any.
    pub clicked: Option<usize>,
    /// Whether the control owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the control.
    pub hovered: bool,
}

/// Creates an equal-width segmented selector.
///
/// Clicking a segment selects it. While focused, ArrowLeft/ArrowUp move to the
/// previous segment, ArrowRight/ArrowDown to the next, Home to the first, and
/// End to the last. Selection clamps rather than wraps.
pub fn segmented(
    context: &mut Context,
    id: &str,
    selected: &mut usize,
    labels: &[&str],
) -> SegmentedResponse {
    context.segmented(id, selected, labels)
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
    fn segmented_clicks_and_arrow_keys_change_selection() {
        let size = Size::new(320.0, 100.0);
        let mut context = Context::new(size);
        let selected = Rc::new(RefCell::new(0_usize));
        let last = Rc::new(RefCell::new(SegmentedResponse::default()));
        let view_selected = Rc::clone(&selected);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            *view_last.borrow_mut() = segmented(
                context,
                "seg",
                &mut view_selected.borrow_mut(),
                &["One", "Two", "Three"],
            );
        };
        click(&mut context, size, "seg:segment:1", &mut build);
        assert_eq!(*selected.borrow(), 1);
        assert!(last.borrow().changed);
        assert!(last.borrow().focused);
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowRight,
            }),
            &mut build,
        );
        assert_eq!(*selected.borrow(), 2);
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowLeft,
            }),
            &mut build,
        );
        assert_eq!(*selected.borrow(), 1);
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::End }),
            &mut build,
        );
        assert_eq!(*selected.borrow(), 2);
    }
}
