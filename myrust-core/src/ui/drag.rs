//! In-window drag source/drop target primitives.

use super::context::Context;
use super::layout::Point;
use super::list::ListRow;

/// Caller-owned drag session.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DragState {
    /// Payload being dragged, if any.
    pub payload: Option<String>,
    /// Source id that started the drag.
    pub origin: Option<String>,
    /// Current pointer position.
    pub current: Option<Point>,
}

impl DragState {
    /// Creates an idle drag state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the active drag.
    pub fn clear(&mut self) {
        self.payload = None;
        self.origin = None;
        self.current = None;
    }
}

/// Result of a completed drop.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DragDrop {
    /// Drop target id.
    pub target: String,
    /// Drag source id.
    pub origin: String,
    /// Dragged payload.
    pub payload: String,
}

/// Result of constructing a drag source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DragSourceResponse {
    /// Whether this source started a drag this frame.
    pub started: bool,
    /// Whether this source is the active drag.
    pub dragging: bool,
}

/// Result of constructing a drop target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DropTargetResponse {
    /// Whether a drag is hovering over this target.
    pub hovered: bool,
    /// Completed drop, if the pointer released here.
    pub dropped: Option<DragDrop>,
}

/// One row in an optional drag-reorder list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListReorderState {
    /// Drag session.
    pub drag: DragState,
    /// Source row index of the last drop.
    pub from: Option<usize>,
    /// Target row index of the last drop.
    pub to: Option<usize>,
}

/// Result of a drag-reorder list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ListReorderResponse {
    /// Source row index, if a row was dropped.
    pub from: Option<usize>,
    /// Target row index, if a row was dropped.
    pub to: Option<usize>,
    /// Whether a row was dropped.
    pub dropped: bool,
}

/// Creates a draggable button.
pub fn drag_source(
    context: &mut Context,
    id: &str,
    label: &str,
    payload: &str,
    state: &mut DragState,
) -> DragSourceResponse {
    context.drag_source(id, label, payload, state)
}

/// Creates a drop target button.
pub fn drop_target(
    context: &mut Context,
    id: &str,
    label: &str,
    state: &mut DragState,
) -> DropTargetResponse {
    context.drop_target(id, label, state)
}

/// Moves an item from `from` to `to`, returning false for equal/out-of-range indices.
pub fn reorder<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from == to || from >= items.len() || to >= items.len() {
        return false;
    }
    let item = items.remove(from);
    items.insert(to, item);
    true
}

/// Creates a simple draggable list that reports source/target row indices.
///
/// The caller applies [`reorder`] to its own row data; this widget only reports
/// the indices and paints the drag highlight.
pub fn reorder_list(
    context: &mut Context,
    id: &str,
    state: &mut ListReorderState,
    rows: &[ListRow<'_>],
    row_height: f32,
) -> ListReorderResponse {
    context.reorder_list(id, state, rows, row_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::event::{PointerButton, UiEvent};
    use crate::ui::layout::{Point, Size};
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

    fn center_of(frame: &super::super::native::ViewFrame, id: &str) -> Point {
        let region = frame
            .interactions
            .iter()
            .find(|region| region.button_id == id)
            .unwrap_or_else(|| panic!("missing interaction region {id}"));
        Point::new(
            region.bounds.x + region.bounds.width / 2.0,
            region.bounds.y + region.bounds.height / 2.0,
        )
    }

    #[test]
    fn drag_source_payload_and_drop_target() {
        let size = Size::new(320.0, 140.0);
        let mut context = Context::new(size);
        let drag = Rc::new(RefCell::new(DragState::new()));
        let drop = Rc::new(RefCell::new(DropTargetResponse::default()));
        let view_drag = Rc::clone(&drag);
        let view_drop = Rc::clone(&drop);
        let mut build = move |context: &mut Context| {
            drag_source(
                context,
                "source",
                "Drag me",
                "alice",
                &mut view_drag.borrow_mut(),
            );
            *view_drop.borrow_mut() =
                drop_target(context, "target", "Drop here", &mut view_drag.borrow_mut());
        };
        let initial = render(&mut context, size, None, &mut build);
        let source = center_of(&initial, "source");
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: source.x,
                y: source.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert_eq!(drag.borrow().payload.as_deref(), Some("alice"));
        assert_eq!(drag.borrow().origin.as_deref(), Some("source"));
        let target = center_of(&initial, "target");
        let moved = render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: target.x,
                y: target.y,
            }),
            &mut build,
        );
        assert_eq!(drag.borrow().current, Some(target));
        assert!(moved.display.commands.iter().any(
            |command| matches!(command, crate::ui::paint::DrawCommand::Text { text, .. } if text == "alice")
        ));
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: target.x,
                y: target.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        let dropped = drop.borrow().dropped.clone().expect("drop result");
        assert_eq!(dropped.target, "target");
        assert_eq!(dropped.origin, "source");
        assert_eq!(dropped.payload, "alice");
        assert!(drag.borrow().payload.is_none());
    }

    #[test]
    fn reorder_list_reports_indices_and_reorders() {
        let size = Size::new(320.0, 180.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(ListReorderState::default()));
        let response = Rc::new(RefCell::new(ListReorderResponse::default()));
        let rows = [
            ListRow::new("a", "Alpha"),
            ListRow::new("b", "Beta"),
            ListRow::new("c", "Gamma"),
        ];
        let view_state = Rc::clone(&state);
        let view_response = Rc::clone(&response);
        let mut build = move |context: &mut Context| {
            *view_response.borrow_mut() =
                reorder_list(context, "rows", &mut view_state.borrow_mut(), &rows, 28.0);
        };
        let initial = render(&mut context, size, None, &mut build);
        let first = center_of(&initial, "rows:row:0");
        let third = center_of(&initial, "rows:row:2");
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: first.x,
                y: first.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerMoved {
                x: third.x,
                y: third.y,
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: third.x,
                y: third.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert!(response.borrow().dropped);
        assert_eq!(response.borrow().from, Some(0));
        assert_eq!(response.borrow().to, Some(2));
        assert_eq!(state.borrow().from, Some(0));
        assert_eq!(state.borrow().to, Some(2));
        let mut items = vec!["a", "b", "c"];
        assert!(reorder(&mut items, 0, 2));
        assert_eq!(items, vec!["b", "c", "a"]);
    }
}
