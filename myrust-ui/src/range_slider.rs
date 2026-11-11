//! Dual-knob range slider.

use super::context::Context;

/// Which range knob is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RangeKnob {
    /// Lower bound.
    Low,
    /// Upper bound.
    #[default]
    High,
}

/// Caller-owned range slider state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeSliderState {
    /// Lower bound.
    pub low: f32,
    /// Upper bound.
    pub high: f32,
    /// Knob adjusted by keyboard input.
    pub active: RangeKnob,
}

impl RangeSliderState {
    /// Creates a range state with both bounds and the high knob active.
    pub fn new(low: f32, high: f32) -> Self {
        Self {
            low,
            high,
            active: RangeKnob::High,
        }
    }
}

impl Default for RangeSliderState {
    fn default() -> Self {
        Self {
            low: 0.0,
            high: 1.0,
            active: RangeKnob::High,
        }
    }
}

/// Result of constructing a range slider for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RangeSliderResponse {
    /// Lower bound after this frame's input.
    pub low: f32,
    /// Upper bound after this frame's input.
    pub high: f32,
    /// Whether either bound changed this frame.
    pub changed: bool,
    /// Knob currently dragged.
    pub dragging: Option<RangeKnob>,
    /// Knob adjusted by keyboard input.
    pub active: RangeKnob,
    /// Whether the slider owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the slider.
    pub hovered: bool,
}

/// Creates a dual-knob range slider.
///
/// Pressing the track picks the nearest knob and drags it. Low is clamped to
/// `high` and high to `low`, so `low <= high` always holds. While focused,
/// ArrowLeft/ArrowDown lower the active knob and ArrowRight/ArrowUp raise it.
pub fn range_slider(
    context: &mut Context,
    id: &str,
    state: &mut RangeSliderState,
    min: f32,
    max: f32,
) -> RangeSliderResponse {
    context.range_slider(id, state, min, max)
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

    fn root_bounds(frame: &super::super::native::ViewFrame) -> crate::paint::Rect {
        frame
            .interactions
            .iter()
            .find(|region| region.button_id == "range")
            .expect("range slider region")
            .bounds
    }

    #[test]
    fn range_slider_drag_clamps_low_and_high_and_keyboard_moves_active_knob() {
        let size = Size::new(300.0, 80.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(RangeSliderState::new(20.0, 80.0)));
        let last = Rc::new(RefCell::new(RangeSliderResponse::default()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            *view_last.borrow_mut() = range_slider(context, "range", &mut state, 0.0, 100.0);
        };

        let initial = render(&mut context, size, None, &mut build);
        let bounds = root_bounds(&initial);
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: bounds.x + 60.0,
                y: bounds.y + 15.0,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: bounds.x + 150.0,
                y: bounds.y + 15.0,
            }),
            &mut build,
        );
        assert!((last.borrow().low - 50.0).abs() < 0.01);
        assert!((last.borrow().high - 80.0).abs() < 0.01);
        assert!(last.borrow().changed);
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: bounds.x + 290.0,
                y: bounds.y + 15.0,
            }),
            &mut build,
        );
        assert!((last.borrow().low - 80.0).abs() < 0.01);
        assert!(
            last.borrow().low <= last.borrow().high,
            "low must clamp to high"
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: bounds.x + 290.0,
                y: bounds.y + 15.0,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert!(!last.borrow().dragging.is_some());

        *state.borrow_mut() = RangeSliderState::new(20.0, 80.0);
        let reset = render(&mut context, size, None, &mut build);
        let bounds = root_bounds(&reset);
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: bounds.x + 240.0,
                y: bounds.y + 15.0,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: bounds.x + 10.0,
                y: bounds.y + 15.0,
            }),
            &mut build,
        );
        assert!((last.borrow().high - 20.0).abs() < 0.01);
        assert!(
            last.borrow().high >= last.borrow().low,
            "high must clamp to low"
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: bounds.x + 10.0,
                y: bounds.y + 15.0,
                button: PointerButton::Left,
            }),
            &mut build,
        );

        *state.borrow_mut() = RangeSliderState::new(20.0, 80.0);
        state.borrow_mut().active = RangeKnob::Low;
        context.request_focus("range");
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowRight,
            }),
            &mut build,
        );
        assert!((last.borrow().low - 21.0).abs() < 0.01);
        state.borrow_mut().active = RangeKnob::High;
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::ArrowUp }),
            &mut build,
        );
        assert!((last.borrow().high - 81.0).abs() < 0.01);
    }
}
