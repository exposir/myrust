`popover(c, "id", &mut state, anchor, content)` is an anchored overlay. It paints above normal content, traps focus while open, and closes on Escape/outside click.

```rust
use myrust_core::ui::OverlayState;

let state = popover(c, "menu", &mut app.popover, anchor, |c| {
    text(c, "Popover content");
});
```

## Without a look

`popover` is an alias for `overlay`: it owns the panel background and anchoring, while content is caller-built. There is no titled popover, arrow, animation, nested popover stack or custom placement.

The anchor must be a previous/current `Rect`; popover width follows the anchor and flips above on overflow. A custom panel chrome can be built inside the content closure.

## Accessibility

The popover surface emits `Role::Popup` with the overlay id; child controls/text keep their nodes. There is no Popover/Dialog role, expanded state, anchor relationship or AT bridge.

Focus moves into the popup layer on open and is restored on close; no platform accessibility bridge consumes it.
