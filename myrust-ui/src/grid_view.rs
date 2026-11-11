//! Scrollable fixed-size grid view.

use super::context::Context;
use super::layout::Size;

/// Caller-owned grid selection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GridViewState {
    /// Selected item index.
    pub selected: Option<usize>,
}

/// Result of constructing a grid view.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GridViewResponse {
    /// Selected item index after this frame.
    pub selected: Option<usize>,
    /// Whether the selection changed.
    pub changed: bool,
    /// Item clicked this frame.
    pub clicked: Option<usize>,
    /// First constructed item index.
    pub visible_start: usize,
    /// One past the last constructed item index.
    pub visible_end: usize,
    /// Scroll offset.
    pub scroll: f32,
    /// Maximum scroll offset.
    pub max_scroll: f32,
}

/// Creates a virtualized, scrollable grid of fixed-size items.
///
/// Only rows intersecting the viewport are constructed. Scrolling uses the
/// shared scroll-view state under the supplied id. `render` is called inside
/// each visible item node with the absolute item index.
#[allow(clippy::too_many_arguments)]
pub fn grid_view(
    context: &mut Context,
    id: &str,
    state: &mut GridViewState,
    item_count: usize,
    item_size: Size,
    columns: usize,
    gap: f32,
    viewport_height: f32,
    render: impl Fn(&mut Context, usize),
) -> GridViewResponse {
    context.grid_view(
        id,
        state,
        item_count,
        item_size,
        columns,
        gap,
        viewport_height,
        render,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Role;
    use crate::container;
    use crate::context::text;
    use crate::event::{PointerButton, UiEvent};
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
    fn grid_view_builds_visible_items_and_selects_after_scrolling() {
        let size = Size::new(320.0, 200.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(GridViewState::default()));
        let last = Rc::new(RefCell::new(GridViewResponse::default()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            container(context).size(300.0, 120.0).children(|context| {
                *view_last.borrow_mut() = grid_view(
                    context,
                    "grid",
                    &mut state,
                    20,
                    Size::new(50.0, 40.0),
                    4,
                    4.0,
                    100.0,
                    |context, index| {
                        text(context, &format!("Item {index}"));
                    },
                );
            });
        };

        let initial = render(&mut context, size, None, &mut build);
        assert_eq!(last.borrow().visible_start, 0);
        assert_eq!(last.borrow().visible_end, 12);
        assert_eq!(
            initial
                .access_nodes
                .iter()
                .filter(|node| node.role == Role::Button)
                .count(),
            12
        );

        let grid_point = center_of(&initial, "grid");
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: grid_point.x,
                y: grid_point.y,
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::Wheel {
                delta_x: 0.0,
                delta_y: 100.0,
            }),
            &mut build,
        );
        let scrolled = render(&mut context, size, None, &mut build);
        assert_eq!(last.borrow().visible_start, 8);
        assert_eq!(last.borrow().visible_end, 20);
        assert_eq!(
            scrolled
                .access_nodes
                .iter()
                .filter(|node| node.role == Role::Button)
                .count(),
            12
        );

        click(&mut context, size, "grid:item:9", &mut build);
        assert_eq!(last.borrow().clicked, Some(9));
        assert_eq!(last.borrow().selected, Some(9));
        assert!(last.borrow().changed);
        assert_eq!(state.borrow().selected, Some(9));
    }
}
