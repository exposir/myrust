`grid_view(c, "id", &mut state, count, item_size, columns, gap, height, render)` is a virtualized, scrollable grid of fixed-size items. `GridViewState { selected: Option<usize> }` is caller-owned.

```rust
use myrust_core::ui::GridViewState;

let state = grid_view(
    c, "photos", &mut app.grid, 40,
    Size::new(64.0, 48.0), 4, 8.0, 220.0,
    |c, i| { text(c, &format!("Photo {i}")); },
);
```

## Grid state

`selected` stores one item index; `GridViewResponse` reports `changed`, `clicked`, `visible_start/end`, `scroll` and `max_scroll`. There is no multi-select, keyboard cursor or item reorder.

## Accessibility

Each constructed item emits `Role::Button`, label `"grid item N"`, value `"selected"`/`"unselected"`. The root is a `Group` with the control id.

There is no Grid/GridCell role, row/column index, item count, position-in-set or AT bridge. Offscreen items are not constructed and therefore absent from nodes.
