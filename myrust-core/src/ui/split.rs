//! Two-panel split view with a draggable divider.

use super::context::Context;
use super::paint::Rect;

/// Minimum fraction of the usable axis assigned to the first panel.
pub const MIN_SPLIT_RATIO: f32 = 0.15;
/// Maximum fraction of the usable axis assigned to the first panel.
pub const MAX_SPLIT_RATIO: f32 = 0.85;

/// Axis along which the two panels are arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitOrientation {
    /// First panel on the left, second on the right, with a vertical divider.
    #[default]
    Horizontal,
    /// First panel on top, second below, with a horizontal divider.
    Vertical,
}

/// Caller-owned split ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitViewState {
    /// Fraction of the usable axis assigned to the first panel.
    pub ratio: f32,
}

impl SplitViewState {
    /// Creates a split state with an explicit ratio.
    pub fn new(ratio: f32) -> Self {
        Self { ratio }
    }
}

impl Default for SplitViewState {
    fn default() -> Self {
        Self { ratio: 0.5 }
    }
}

/// Result of constructing a split view for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SplitViewResponse {
    /// Ratio after this frame's drag or keyboard input.
    pub ratio: f32,
    /// Whether the ratio changed this frame.
    pub changed: bool,
    /// Whether the divider is being dragged.
    pub dragging: bool,
    /// Whether the divider owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the divider.
    pub hovered: bool,
    /// Previous-frame divider bounds, when available.
    pub divider_bounds: Option<Rect>,
}

/// Creates a two-panel split view that fills the current content box.
///
/// The divider is draggable with the primary pointer and adjustable with
/// ArrowLeft/ArrowUp (smaller first panel) and ArrowRight/ArrowDown (larger
/// first panel) when focused. The ratio is clamped to
/// [`MIN_SPLIT_RATIO`]..=[`MAX_SPLIT_RATIO`]. Place it inside a container with
/// an explicit height; otherwise the current window height is used.
pub fn split_view(
    context: &mut Context,
    id: &str,
    state: &mut SplitViewState,
    orientation: SplitOrientation,
    first: impl FnOnce(&mut Context),
    second: impl FnOnce(&mut Context),
) -> SplitViewResponse {
    context.split_view(id, state, orientation, first, second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::container;
    use crate::ui::context::text;
    use crate::ui::event::{PointerButton, UiEvent};
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

    #[test]
    fn horizontal_split_drag_updates_ratio_and_clamps_both_ends() {
        let size = Size::new(420.0, 260.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(SplitViewState::default()));
        let last = Rc::new(RefCell::new(SplitViewResponse::default()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            container(context).size(400.0, 200.0).children(|context| {
                *view_last.borrow_mut() = split_view(
                    context,
                    "split",
                    &mut state,
                    SplitOrientation::Horizontal,
                    |context| {
                        text(context, "Left panel");
                    },
                    |context| {
                        text(context, "Right panel");
                    },
                );
            });
        };

        let initial = render(&mut context, size, None, &mut build);
        let start = center_of(&initial, "split:divider");
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: start.x,
                y: start.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert!(last.borrow().dragging);
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: 300.0,
                y: start.y,
            }),
            &mut build,
        );
        assert!(last.borrow().changed);
        assert!(
            (last.borrow().ratio - 0.7538).abs() < 0.01,
            "ratio was {}",
            last.borrow().ratio
        );

        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: 500.0,
                y: start.y,
            }),
            &mut build,
        );
        assert!((last.borrow().ratio - MAX_SPLIT_RATIO).abs() < 0.001);
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: -100.0,
                y: start.y,
            }),
            &mut build,
        );
        assert!((last.borrow().ratio - MIN_SPLIT_RATIO).abs() < 0.001);
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: -100.0,
                y: start.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert!(!last.borrow().dragging);
        assert!((state.borrow().ratio - MIN_SPLIT_RATIO).abs() < 0.001);

        let after = render(&mut context, size, None, &mut build);
        let start = center_of(&after, "split:divider");
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: start.x,
                y: start.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: 500.0,
                y: start.y,
            }),
            &mut build,
        );
        assert!((last.borrow().ratio - MAX_SPLIT_RATIO).abs() < 0.001);
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: 500.0,
                y: start.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
    }

    #[test]
    fn vertical_split_drag_updates_ratio() {
        let size = Size::new(340.0, 260.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(SplitViewState::default()));
        let last = Rc::new(RefCell::new(SplitViewResponse::default()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            container(context).size(300.0, 200.0).children(|context| {
                *view_last.borrow_mut() = split_view(
                    context,
                    "split-v",
                    &mut state,
                    SplitOrientation::Vertical,
                    |context| {
                        text(context, "Top panel");
                    },
                    |context| {
                        text(context, "Bottom panel");
                    },
                );
            });
        };
        let initial = render(&mut context, size, None, &mut build);
        let start = center_of(&initial, "split-v:divider");
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: start.x,
                y: start.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: start.x,
                y: 160.0,
            }),
            &mut build,
        );
        assert!(
            (last.borrow().ratio - 0.8093).abs() < 0.01,
            "ratio was {}",
            last.borrow().ratio
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: start.x,
                y: 160.0,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert!(!last.borrow().dragging);
        assert!((state.borrow().ratio - 0.8093).abs() < 0.01);

        context.request_focus("split-v:divider");
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: crate::ui::event::Key::ArrowRight,
            }),
            &mut build,
        );
        assert!(last.borrow().changed);
        assert!((last.borrow().ratio - 0.8293).abs() < 0.01);
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: crate::ui::event::Key::ArrowLeft,
            }),
            &mut build,
        );
        assert!((last.borrow().ratio - 0.8093).abs() < 0.01);
    }
}
