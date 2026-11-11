//! Data table with sortable headers and row selection.

use super::context::Context;

/// Sort direction requested by a table header click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    /// Ascending order.
    #[default]
    Ascending,
    /// Descending order.
    Descending,
}

/// One table column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableColumn<'a> {
    /// Header title.
    pub title: &'a str,
    /// Column width in logical pixels.
    pub width: f32,
}

impl<'a> TableColumn<'a> {
    /// Creates a table column.
    pub const fn new(title: &'a str, width: f32) -> Self {
        Self { title, width }
    }
}

/// One table row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableRow<'a> {
    /// Stable row id returned on selection.
    pub id: &'a str,
    /// Cell text, one entry per column.
    pub cells: &'a [&'a str],
}

impl<'a> TableRow<'a> {
    /// Creates a table row.
    pub const fn new(id: &'a str, cells: &'a [&'a str]) -> Self {
        Self { id, cells }
    }
}

/// Caller-owned table selection and sort state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableState {
    /// Selected row id.
    pub selected_id: String,
    /// Column currently sorted, if any.
    pub sort_column: Option<usize>,
    /// Current sort direction.
    pub sort_direction: SortDirection,
}

impl TableState {
    /// Creates an empty table state.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A sort request emitted by a header click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSort {
    /// Column index.
    pub column: usize,
    /// Requested direction.
    pub direction: SortDirection,
}

/// Result of constructing a table for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableResponse {
    /// Selected row id after this frame's click input.
    pub selected_id: String,
    /// Whether the selected row changed this frame.
    pub changed: bool,
    /// Row id clicked this frame, if any.
    pub clicked_id: Option<String>,
    /// Header sort request this frame, if any.
    pub sort: Option<TableSort>,
}

/// Creates a table. The caller sorts its row data according to the returned
/// [`TableSort`] before the next frame.
pub fn table(
    context: &mut Context,
    id: &str,
    state: &mut TableState,
    columns: &[TableColumn<'_>],
    rows: &[TableRow<'_>],
) -> TableResponse {
    context.table(id, state, columns, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Role;
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
    fn table_headers_emit_sort_requests_and_rows_select() {
        let size = Size::new(420.0, 240.0);
        let columns = [
            TableColumn::new("Name", 140.0),
            TableColumn::new("Kind", 100.0),
        ];
        let row_a = ["Alpha", "File"];
        let row_b = ["Beta", "Folder"];
        let row_c = ["Gamma", "File"];
        let rows = [
            TableRow::new("r0", &row_a),
            TableRow::new("r1", &row_b),
            TableRow::new("r2", &row_c),
        ];
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(TableState::new()));
        let last = Rc::new(RefCell::new(TableResponse::default()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            *view_last.borrow_mut() = table(context, "table", &mut state, &columns, &rows);
        };

        let initial = render(&mut context, size, None, &mut build);
        assert!(initial
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Button && node.label == "Name"));

        click(&mut context, size, "table:head:0", &mut build);
        let first = last.borrow().sort.expect("sort request");
        assert_eq!(first.column, 0);
        assert_eq!(first.direction, SortDirection::Ascending);
        click(&mut context, size, "table:head:0", &mut build);
        let second = last.borrow().sort.expect("sort request");
        assert_eq!(second.column, 0);
        assert_eq!(second.direction, SortDirection::Descending);
        click(&mut context, size, "table:head:1", &mut build);
        let third = last.borrow().sort.expect("sort request");
        assert_eq!(third.column, 1);
        assert_eq!(third.direction, SortDirection::Ascending);

        click(&mut context, size, "table:row:1", &mut build);
        assert_eq!(last.borrow().selected_id, "r1");
        assert_eq!(last.borrow().clicked_id.as_deref(), Some("r1"));
        assert!(last.borrow().changed);
        assert_eq!(state.borrow().selected_id.as_str(), "r1");
    }
}
