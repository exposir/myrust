`switch(c, "id", &mut on)` draws a toggle switch and returns `SwitchState { changed, checked, focused, hovered }`. Click or Space toggles the caller boolean.

```rust
let state = switch(c, "wifi", &mut app.wifi);
if state.changed {
    app.apply_network();
}
```

## Without a look

There is no low-level track/thumb builder. Custom visuals need `container`/`draw` and their own interaction; the standard switch is the only control that emits the Switch AccessNode.

## Accessibility

The switch emits `Role::Switch`, label `"switch"`, value `"on"`/`"off"`, and focused boolean. It has no visible label of its own beyond the id argument, no description/error and no disabled state; no AT bridge exists.
