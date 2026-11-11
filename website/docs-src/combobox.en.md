`combobox(c, "id", &mut query, &mut state, &["Apple", "Apricot"])` is an editable text input with a filtered option overlay. Typing filters case-insensitively; Enter selects; Escape closes.

```rust
use myrust_core::ui::ComboboxState;

let state = combobox(
    c, "fruit", &mut app.query, &mut app.combo,
    &["Apple", "Apricot", "Banana"],
);
if let Some(index) = state.selected {
    app.choose(index);
}
```

## Without a look

There is no low-level combobox/listbox builder or custom option row. The trigger is the text input itself; options are drawn in an overlay with the shared primitive.

No disabled option, grouping, multi-select, async loading or option icons exist. `select` is the non-editable index picker; `autocomplete` only suggests text.

## Accessibility

The input emits `Role::TextField`, label `"text field"`, value = query, focused boolean. Option rows are ordinary `Role::Label` text nodes, not `Option`; there is no ComboBox role, expanded state, active descendant or AT bridge.
