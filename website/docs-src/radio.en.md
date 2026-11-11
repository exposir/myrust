`radio_group(c, "size", &mut selected, &["Small", "Medium"])` draws a vertical radio list and keeps one selected index.

```rust
let state = radio_group(c, "size", &mut app.size, &["Small", "Medium", "Large"]);
if state.changed {
    app.apply_size(state.selected);
}
```

## Without a look

There is no low-level radio builder or custom radio row. `radio_group` is the public primitive; a custom visual needs a separate container and manual selection/key handling.

## Accessibility

The root emits `Role::RadioGroup` with the control id as label and focused boolean. Each option emits `Role::Radio` with its label and value `"selected"`/`"unselected"`; options are not separately focusable.

No disabled option, multi-select, description/error or AT bridge exists.
