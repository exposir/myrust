`time_input(c, "id", &mut text)` 显示 `HH:MM` 文本字段与四个步进按钮。文本过滤为数字与一个冒号；Enter 规范化合法值。

```rust
let state = time_input(c, "time", &mut app.time_text);
if state.valid {
    app.hour = state.hour;
    app.minute = state.minute;
}
```

## Accessibility

根发出 `Role::Group`，label 为 id。input 发出 `Role::TextField`，label 为 `"text field"`，value 为 `HH:MM`，带 focused 布尔。

四个步进控件发出 `Role::Button`，label 为内部 key（`"<id>:hour_dec"`、`"<id>:minute_inc"` 等），不是友好文本。没有 Time/SpinButton role、min/max、invalid/error 或 AT 桥。
