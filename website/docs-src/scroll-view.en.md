`scroll_view(c, "id", viewport_height, content)` clips a vertical viewport and offsets its content. Wheel, scrollbar drag and keyboard scrolling are internal.

```rust
let state = scroll_view(c, "panel", 220.0, |c| {
    for row in 0..20 { text(c, &format!("Row {row}")); }
});
```

## Keeping the offset

Offset is context-owned under the scroll id; `ScrollState` returns `scroll`, `max_scroll`, `changed`, `dragging`, `hovered`. Rebuilding content keeps the offset unless the scroll id changes.

## Sticky elements

No sticky header/row exists. A caller can draw a fixed header outside the scroll view; there is no sticky-within-scroll behavior.

## Scrolling into view

No `scroll_into_view` API. Callers can set context scroll state only through internals; lists/grid views compute offsets automatically, arbitrary scroll views cannot be programmatically scrolled by public API.

## Snapping

No snap points, paging, momentum or smooth wheel animation exists. Wheel deltas apply directly.

## Scroll bars

One internal vertical scrollbar is drawn; there are no horizontal bars, corner, style hooks or always-visible options. Overlay scrollbars are not configurable.

## Accessibility

The viewport emits `Role::ScrollView`, label `"scroll view"`, no value. Children keep their own roles. There is no scroll position/range metadata, focus-follow or AT bridge.
