`tooltip_for(c, "id", anchor, target_id, text)` shows a small label when the pointer hovers the target or the target has focus, after a delay. `tooltip` is the anchor-only variant.

```rust
let tip = tooltip_for(c, "hint", anchor, "save-button", "Save changes");
if tip.visible {
    app.tooltip_seen = true;
}
```

## The delay

The default delay is `TOOLTIP_DELAY`; `tooltip_with_delay` accepts a custom `Duration`. Hover/focus starts the timer; leaving clears it.

No tooltip follows the pointer, flips automatically, supports rich content or has a close button. The delay is wall-clock, so tests freeze the frame clock or wait.

## Without a look

There is no low-level tooltip body builder, arrow, animation or collision engine. `tooltip_for` draws a rounded label box; custom content needs an overlay (but then it is no longer a tooltip).

## Accessibility

The tooltip text is a `Role::Label` node with no value; the anchor target emits its own node. There is no Tooltip role, `aria-describedby` relationship, delay metadata or AT bridge.

Tooltips are not announced by a screen reader in this implementation.
