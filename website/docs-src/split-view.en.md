`split_view(c, "id", &mut state, orientation, first, second)` lays two panels with a draggable divider. `SplitViewState { ratio }` is caller-owned; arrows move the focused divider by 0.02.

```rust
use myrust_core::ui::{SplitOrientation, SplitViewState};

let response = split_view(c, "split", &mut app.split, SplitOrientation::Horizontal, |c| {
    text(c, "Left");
}, |c| {
    text(c, "Right");
});
```

## Accessibility

The divider emits `Role::Slider`, label `"split view"`, value `"N%"`, focused boolean. Panel roots are groups; there are no separate panel roles.

There is no min/max panel size, double-click reset, keyboard Home/End or AT bridge. The response exposes ratio/changed/dragging/focused/hovered/divider_bounds.
