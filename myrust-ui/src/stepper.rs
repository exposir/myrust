//! Integer stepper with minus/plus buttons.

use super::context::Context;

/// Result of constructing a stepper for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StepperState {
    /// Whether the value changed this frame.
    pub changed: bool,
    /// Value after this frame's input.
    pub value: i64,
    /// Whether the stepper owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the stepper.
    pub hovered: bool,
}

/// Creates an integer stepper.
///
/// The value is clamped to `min..=max`; `step` is forced to at least one.
/// Minus/plus buttons step by `step`, and ArrowUp/ArrowDown do the same while
/// the control is focused.
pub fn stepper(
    context: &mut Context,
    id: &str,
    value: &mut i64,
    min: i64,
    max: i64,
    step: i64,
) -> StepperState {
    context.stepper(id, value, min, max, step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Key, PointerButton, UiEvent};
    use crate::layout::Size;
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
    fn stepper_clicks_and_arrow_keys_step_and_clamp() {
        let size = Size::new(260.0, 100.0);
        let mut context = Context::new(size);
        let value = Rc::new(RefCell::new(5_i64));
        let last = Rc::new(RefCell::new(StepperState::default()));
        let view_value = Rc::clone(&value);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut value = view_value.borrow_mut();
            *view_last.borrow_mut() = stepper(context, "step", &mut value, 0, 10, 2);
        };

        click(&mut context, size, "step:inc", &mut build);
        assert_eq!(*value.borrow(), 7);
        assert!(last.borrow().changed);
        click(&mut context, size, "step:inc", &mut build);
        assert_eq!(*value.borrow(), 9);
        click(&mut context, size, "step:inc", &mut build);
        assert_eq!(*value.borrow(), 10);
        click(&mut context, size, "step:inc", &mut build);
        assert_eq!(*value.borrow(), 10, "step clamps at max");
        assert!(!last.borrow().changed);

        click(&mut context, size, "step:dec", &mut build);
        assert_eq!(*value.borrow(), 8);
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::ArrowUp }),
            &mut build,
        );
        assert_eq!(*value.borrow(), 10);
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        assert_eq!(*value.borrow(), 8);

        *value.borrow_mut() = 1;
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        assert_eq!(*value.borrow(), 0);
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        assert_eq!(*value.borrow(), 0, "step clamps at min");
    }
}
