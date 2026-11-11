//! Virtualized vertical list.
//!
//! The list reuses the scroll-view state (`offset` and wheel handling) under
//! its id. Only rows intersecting the viewport are constructed each frame;
//! zero-height spacer nodes preserve the total scroll height.

use super::context::Context;

/// One list row. A missing height falls back to the list's default height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListRow<'a> {
    /// Stable row id returned on selection.
    pub id: &'a str,
    /// Visible row label.
    pub label: &'a str,
    /// Explicit row height, or `None` for the list default.
    pub height: Option<f32>,
}

impl<'a> ListRow<'a> {
    /// Creates a row using the default height.
    pub const fn new(id: &'a str, label: &'a str) -> Self {
        Self {
            id,
            label,
            height: None,
        }
    }

    /// Sets an explicit row height.
    pub const fn with_height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }
}

/// Caller-owned list selection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListState {
    /// Selected row id.
    pub selected_id: String,
}

impl ListState {
    /// Creates an empty selection.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Result of constructing a virtualized list for one frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListResponse {
    /// Selected row id after this frame's click input.
    pub selected_id: String,
    /// Whether the selection changed this frame.
    pub changed: bool,
    /// Row id clicked this frame, if any.
    pub clicked_id: Option<String>,
    /// First constructed row index.
    pub visible_start: usize,
    /// One past the last constructed row index.
    pub visible_end: usize,
    /// Scroll offset applied by the reused scroll-view state.
    pub scroll: f32,
    /// Maximum valid scroll offset for the current row heights.
    pub max_scroll: f32,
}

/// Creates a virtualized vertical list.
///
/// `default_height` is used by rows without an explicit height.
/// `viewport_height` is clamped to at least one pixel. Scrolling uses the same
/// context-owned scroll state as [`super::context::Context::scroll_view`],
/// under the supplied list id.
pub fn list(
    context: &mut Context,
    id: &str,
    state: &mut ListState,
    rows: &[ListRow<'_>],
    default_height: f32,
    viewport_height: f32,
) -> ListResponse {
    context.list(id, state, rows, default_height, viewport_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::access::Role;
    use crate::ui::container;
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
    fn list_builds_only_visible_rows_and_selects_after_scrolling() {
        let size = Size::new(340.0, 200.0);
        let entries = (0..100)
            .map(|index| (format!("row-{index}"), format!("Row {index}")))
            .collect::<Vec<_>>();
        let rows = entries
            .iter()
            .map(|(id, label)| ListRow::new(id.as_str(), label.as_str()))
            .collect::<Vec<_>>();
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(ListState::new()));
        let last = Rc::new(RefCell::new(ListResponse::default()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            container(context).size(300.0, 120.0).children(|context| {
                *view_last.borrow_mut() = list(context, "list", &mut state, &rows, 20.0, 100.0);
            });
        };

        let initial = render(&mut context, size, None, &mut build);
        assert_eq!(last.borrow().visible_start, 0);
        assert_eq!(last.borrow().visible_end, 5);
        assert_eq!(last.borrow().scroll, 0.0);
        assert_eq!(
            initial
                .access_nodes
                .iter()
                .filter(|node| node.role == Role::Option)
                .count(),
            5
        );
        assert!(!initial
            .display
            .commands
            .iter()
            .any(|command| matches!(command, crate::ui::paint::DrawCommand::Text { text, .. } if text == "Row 50")));

        let list_point = center_of(&initial, "list");
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: list_point.x,
                y: list_point.y,
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::Wheel {
                delta_x: 0.0,
                delta_y: 200.0,
            }),
            &mut build,
        );
        assert!(last.borrow().scroll >= 200.0);

        let scrolled = render(&mut context, size, None, &mut build);
        assert_eq!(last.borrow().visible_start, 10);
        assert_eq!(last.borrow().visible_end, 15);
        assert!(scrolled
            .display
            .commands
            .iter()
            .any(|command| matches!(command, crate::ui::paint::DrawCommand::Text { text, .. } if text == "Row 10")));
        assert!(!scrolled
            .display
            .commands
            .iter()
            .any(|command| matches!(command, crate::ui::paint::DrawCommand::Text { text, .. } if text == "Row 0")));

        click(&mut context, size, "list:list:row:12", &mut build);
        assert_eq!(last.borrow().selected_id, "row-12");
        assert_eq!(last.borrow().clicked_id.as_deref(), Some("row-12"));
        assert!(last.borrow().changed);
        assert_eq!(state.borrow().selected_id.as_str(), "row-12");
    }
}

#[cfg(test)]
mod variable_height_tests {
    use super::*;
    use crate::ui::container;
    use crate::ui::layout::Size;
    use std::cell::RefCell;

    #[test]
    fn list_uses_per_row_heights_for_the_visible_window() {
        let size = Size::new(300.0, 180.0);
        let mut context = Context::new(size);
        let rows = [
            ListRow::new("a", "A").with_height(30.0),
            ListRow::new("b", "B").with_height(50.0),
            ListRow::new("c", "C").with_height(20.0),
            ListRow::new("d", "D").with_height(40.0),
        ];
        let state = RefCell::new(ListState::new());
        let last = RefCell::new(ListResponse::default());
        let build = |context: &mut Context| {
            let mut state = state.borrow_mut();
            container(context).size(260.0, 100.0).children(|context| {
                *last.borrow_mut() = list(context, "variable-list", &mut state, &rows, 28.0, 70.0);
            });
        };
        context.begin_frame(size);
        build(&mut context);
        let frame = context.produce().unwrap();
        context.end_frame();
        assert_eq!(last.borrow().visible_start, 0);
        assert_eq!(
            last.borrow().visible_end,
            2,
            "row A is 30 and row B is 50; row C starts below the 70px viewport"
        );
        assert_eq!(
            frame
                .access_nodes
                .iter()
                .filter(|node| node.role == crate::ui::access::Role::Option)
                .count(),
            2
        );
        assert!(!frame.display.commands.iter().any(
            |command| matches!(command, crate::ui::paint::DrawCommand::Text { text, .. } if text == "C")
        ));
    }
}
