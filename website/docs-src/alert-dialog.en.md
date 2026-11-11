`alert_dialog(c, "id", &mut open, title, message, confirm_label, cancel_label)` is the fixed-shape confirmation dialog. `DialogResponse` reports confirm/cancel/escape.

```rust
let response = alert_dialog(
    c, "delete", &mut app.open,
    "Delete?", "Cannot undo", "Delete", "Cancel",
);
if response.confirmed {
    app.delete();
}
```

## Accessibility

The panel emits `Role::Popup` with the alert id. Title/message are `Role::Label`s; confirm/cancel actions are `Role::Button` with their visible labels.

There is no AlertDialog role, danger styling, required checkbox or AT bridge. Focus is trapped in the popup layer and restored on close.
