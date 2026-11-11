`list(c, "id", &mut state, rows, row_height, viewport_height)` is a virtualized vertical list of `ListRow { id, label, height }`. Only rows intersecting the viewport are constructed.

```rust
use myrust_core::ui::{ListRow, ListState, list};

let rows = [ListRow::new("a", "Alpha"), ListRow::new("b", "Beta")];
let state = list(c, "rows", &mut app.list, &rows, 28.0, 160.0);
```

## Layout

Rows are full-width with caller/default height; the list is vertical and non-wrapping. There is no horizontal list, multi-column list or sticky row.

## Rows wider than the list

Content is clipped by the viewport; there is no horizontal scroll, ellipsis or text wrapping per row. Wider row content is caller responsibility.

## List state

`ListState` holds only `selected_id`. Scroll offset lives in context under the list id; `ListResponse` reports `visible_start/end`, `scroll`, `max_scroll`.

## Choosing rows

A row click sets `selected_id`; `changed` and `clicked_id` report it. There is no multi-select, checkbox mode, range select or keyboard row navigation.

## Scrolling

Wheel, scrollbar drag and keyboard scrolling reuse `scroll_view` state. Lists do not own a scroll controller; `scroll`, `max_scroll` are returned.

## Reordering

The list itself does not reorder rows. `reorder_list` is a separate helper with drag state; `reorder` applies indices to caller data.

## Dragging rows out

No OS drag, cross-window row drag or drop representation exists. In-window reorder is the only row drag path.

## The focus

Rows are not focusable individually; the list is not a focus stop. There is no arrow-key row cursor or selection announcement.

## Accessibility

Each constructed row emits `Role::Option`, label = row label, value `"selected"`/`"unselected"`. There is no list/table role, row count, position-in-set, multi-select or AT bridge.
