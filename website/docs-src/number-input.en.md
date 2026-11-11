`number_input(c, "id", &mut text, options)` is a text field that filters to numeric characters and steps with ArrowUp/ArrowDown.

```rust
use myrust_core::ui::NumberInputOptions;

let state = number_input(
    c,
    "count",
    &mut app.count_text,
    NumberInputOptions::default().allow_negative(true).decimals(0).step(1.0),
);
if state.changed {
    app.count = state.value;
}
```

## Accessibility

The underlying field emits `Role::TextField`, label `"text field"`, value = the visible numeric string, focused boolean. There is no `SpinButton`/`NumberInput` role, min/max metadata, step metadata or error/invalid state.

No AT bridge consumes this. A custom accessible numeric control would need a new role/API; `number_input` is a text field with filtering, not a spinbutton.
