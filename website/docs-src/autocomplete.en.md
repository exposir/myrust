`autocomplete(c, "id", &mut text, &mut state, candidates)` suggests candidates below a text field while the user types. Enter selects the highlighted candidate; Escape dismisses the overlay.

```rust
use myrust_core::ui::AutocompleteState;

let state = autocomplete(
    c, "city", &mut app.city, &mut app.auto,
    &["Berlin", "Bern", "Boston"],
);
if let Some(index) = state.selected {
    app.choose(index);
}
```

## Accessibility

The input emits `Role::TextField`, label `"text field"`, value = text, focused boolean. Suggestion rows are ordinary `Role::Label` text nodes, not `Option`; there is no ComboBox/listbox role, active descendant or expanded state.

No async suggestions, loading state, disabled candidate or AT bridge exists. The popup is visual-only metadata.
