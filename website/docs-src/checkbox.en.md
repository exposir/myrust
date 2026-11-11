`checkbox(c, "Accept terms", &mut checked)` draws a labeled checkbox and returns `CheckboxState { changed, checked, focused, hovered }`. Click or Space toggles it.

```rust
let state = checkbox(c, "Accept terms", &mut app.accepted);
if state.changed {
    app.validate();
}
```

## Groups

There is no checkbox-group or multi-select container. Each checkbox owns one caller `bool`; grouping is visual/application state only.

The `id` is also the visible label. There is no separate label/description/error API, no disabled checkbox and no group-level AccessNode.

## Mixed

Only a boolean is supported. There is no mixed/indeterminate tri-state, “select all” helper or per-child aggregation; a parent checkbox must compute its own children state.

## Without a look

There is no low-level checkbox builder. A custom visual needs `container`/`draw` plus manual hit handling; the standard control emits the checkbox AccessNode and focus behavior.

## Accessibility

The checkbox emits `Role::CheckBox`, label `"checkbox"` (not the visible id), value `"checked"`/`"unchecked"`, and the focused boolean. There is no mixed state, description, error, disabled flag or AT bridge.
