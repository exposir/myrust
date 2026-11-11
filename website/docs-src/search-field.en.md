`search_field(c, "id", &mut query)` is a text field with a trailing clear button. Enter returns `submitted`; Escape or the clear button returns `cleared`.

```rust
let state = search_field(c, "find", &mut app.query);
if state.submitted {
    app.search();
}
```

## Accessibility

The root emits `Role::Group` with the control id as label. The input emits `Role::TextField`, label `"text field"`, value = query, focused boolean; the clear button emits `Role::Button`, label `"clear search"`, no value.

There is no Search/SearchBox role, description, error, results count or AT bridge. The clear button is not focusable separately.
