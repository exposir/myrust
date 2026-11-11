`find_bar(c, "id", &mut query, &mut current, matches)` draws a search input, a match count and previous/next buttons. `current` is clamped; navigation wraps when there are matches.

```rust
let state = find_bar(c, "find", &mut app.find_query, &mut app.find_index, app.matches);
if state.changed {
    app.highlight_match(state.current);
}
```

## Keyboard

The input uses the normal text-field editor. The bar itself has no Enter-to-next, Escape-to-close, arrow-match navigation or search shortcut; previous/next are pointer buttons, and zero matches disables them.

Tab can focus the input and buttons according to construction order. There is no `Cmd+F`/focus shortcut owned by the widget; the host/app must open it.

## Accessibility

The root emits `Role::Group` with the control id. The input is `Role::TextField`, label `"text field"`, value = query, focused boolean. Navigation buttons are `Role::Button`, labels `"previous match"`/`"next match"`, value `"enabled"`/`"disabled"`; the count is a `Role::Label` text node.

There is no search-results/list role, current/total semantic metadata, error state or AT bridge.
