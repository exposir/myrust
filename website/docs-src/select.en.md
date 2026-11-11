`select(c, "id", &mut selected, &["One", "Two"])` is a drop-down trigger with an overlay option list. The selected index is caller-owned.

```rust
let state = select(c, "mode", &mut app.mode, &["Light", "Dark", "System"]);
if state.changed {
    app.apply_mode(state.selected);
}
```

## Without a look

There is no low-level select/listbox builder or custom option row. `select` owns the trigger, overlay and option rendering; custom content needs a separate overlay and manual controls.

The trigger has no hint text, disabled state, search filtering or multi-select. `combobox` handles filtered text choices; `select` is the simple index picker.

## Accessibility

The trigger emits `Role::ComboBox`, label `"select"`, value = the selected option label, and focused boolean. Open options emit `Role::Option` with option text and value `"selected"`/`"unselected"`.

There is no expanded/collapsed state, active descendant, disabled option or AT bridge.
