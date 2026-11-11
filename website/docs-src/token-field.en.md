`token_field(c, "id", &mut tokens, &mut input)` shows strings as chips with an inline input. Enter adds the buffer; comma splits; Backspace on empty removes the last; clicking a chip removes it.

```rust
let state = token_field(c, "tags", &mut app.tags, &mut app.tag_input);
if state.added {
    app.sync_tags();
}
```

## Accessibility

The root emits `Role::Group` with the control id as label. Each chip is a `Role::Button`, label `"remove token <text>"`, no value. The input emits `Role::TextField`, label `"text field"`, value = buffer, focused boolean.

There is no token/list role, chip text as a separate label, set position/size, disabled chip or AT bridge. Removing via keyboard is limited to Backspace on an empty input.
