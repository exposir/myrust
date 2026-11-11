`back_forward_buttons(c, "id", &mut RouterState)` draws two history buttons and moves the router index on click. It is the UI companion to `RouterState`.

```rust
let state = back_forward_buttons(c, "history", &mut app.router);
if state.back_clicked {
    app.leave_page();
}
```

## Accessibility

Both controls emit `Role::Button`: labels `"back"` and `"forward"`, values `"enabled"`/`"disabled"`, no focused flag on disabled buttons.

There is no History/Navigation role, shortcut metadata, page title announcement or AT bridge. Disabled buttons are removed from interaction but still emit nodes.
