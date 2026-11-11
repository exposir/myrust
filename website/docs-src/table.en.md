`table(c, "id", &mut state, columns, rows)` draws fixed-width columns and selectable rows. Sorting is a request returned to the caller; the table does not reorder data.

```rust
use myrust_core::ui::{table, TableColumn, TableRow, TableState};

let columns = [TableColumn::new("Name", 120.0)];
let rows = [TableRow::new("a", &["Alpha"]), TableRow::new("b", &["Beta"])];
let response = table(c, "t", &mut app.table, &columns, &rows);
```

## Choosing rows

Row clicks update `TableState.selected_id`; `TableResponse` reports `selected_id`, `changed`, `clicked_id`. There is no multi-row selection.

## Choosing cells

Cells are not individually selectable or focusable; there is no cell cursor, range selection or copy grid.

## Arranging columns

Column widths are explicit `TableColumn::width`; rows fill cells. There is no drag resize, column reorder, hide/show or proportional width.

## Sorting

A header click returns `TableSort { column, direction }` and updates `state.sort_column/sort_direction`. The caller must sort rows before the next frame; no comparator is stored.

## Header menu

No per-column header menu (hide, pin, filter) exists. Header buttons only request sorting.

## Header cells

Headers are `Role::Button` with the title as label and value `ascending`/`descending`/`none` for the sorted state. They are not separately focusable per column.

## Cell menu

No context menu, edit, copy or row action menu exists. Add a separate `context_menu` with the table row's bounds.

## Renaming in place

No inline cell editor. Compose `editable_text` or a text field into a custom row/cell outside the table.

## Accessibility

Rows emit `Role::Option`, label = first cell or row id, value `"selected"`/`"unselected"`. Headers emit `Role::Button`; the root emits `Role::Group` with the table id.

There is no Table/Row/Cell role, column index, sort state semantics beyond header label/value, or AT bridge.
