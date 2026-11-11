`rating(c, "stars", &mut value, 5)` draws five star buttons and sets a `u32` from zero to max. Clicking star `n` sets `n`; hovering previews that value.

```rust
let state = rating(c, "quality", &mut app.rating, 5);
if state.changed {
    app.review(state.value);
}
```

## Accessibility

The root emits `Role::Slider`, label `"rating"`, value = `"n/max"`, and focused boolean. Each star emits `Role::Button`, label `"star 1"`…, value `"filled"`/`"empty"`; stars are not separately focusable.

There is no rating/label role, half-star value, description, disabled state or AT bridge. Keyboard arrows change the root value.
