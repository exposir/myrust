`alert_dialog(c, "id", &mut open, title, message, confirm_label, cancel_label)` 是固定形状确认对话框。`DialogResponse` 报告 confirm/cancel/escape。

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

panel 发出 `Role::Popup`，label 为 alert id。title/message 是 `Role::Label`；confirm/cancel 动作是 `Role::Button`，label 为可见文本。

没有 AlertDialog role、danger styling、required checkbox 或 AT 桥。焦点限制在 popup layer，关闭后恢复。
