`stepper(c, "count", &mut value, 0, 10, 1)` draws minus/plus buttons and a centered value. The integer is clamped to the range; step is forced to at least one.

```rust
let state = stepper(c, "count", &mut app.count, 0, 10, 1);
if state.changed {
    app.save_count();
}
```

## Accessibility

The root emits `Role::Slider`, label `"stepper"`, value = the current integer, and focused boolean. The minus/plus buttons emit `Role::Button`, labels `"-"`/`"+"`, value `None`.

There is no spinbutton role, min/max/step metadata, visible label, disabled state or AT bridge. ArrowUp/ArrowDown step while the root is focused.
